//! One window's typed drag session. A host handing it to the platform as it
//! leaves the window (to other apps) reads what it carries from
//! `drag_state`, and tells it how it ended (`end_drag_elsewhere`).
use super::*;
use crate::actions::Action;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragPhase {
    Start,
    Move,
    Over,
    Leave,
    Drop,
    End,
}
#[derive(Clone, Debug, PartialEq)]
pub struct DragEvent {
    pub phase: DragPhase,
    pub payload: Action,
    pub source: NodeId,
    pub x: f32,
    pub y: f32,
    /// Pointer position within the source's bounds when the gesture was pressed.
    /// Subtracting it from `x`/`y` keeps a preview under the original grab point.
    pub grab_x: f32,
    pub grab_y: f32,
    /// True on End only when the accepted destination consumed Drop.
    pub accepted: bool,
}
/// What a drag carries to other apps, when a host hands it to the platform
/// as it leaves the window (`View::drag_export`).
#[derive(Clone, Debug, PartialEq)]
pub enum DragItem {
    Text(String),
    Url(String),
    File(std::path::PathBuf),
    /// An encoded image (PNG, JPEG, TIFF).
    Image(Rc<[u8]>),
}
/// A window's drag, as its host reads it (`InputDispatcher::drag_state`).
#[derive(Clone, Debug)]
pub struct DragState {
    pub payload: Action,
    pub source: NodeId,
    /// The pointer, in the window's coordinates.
    pub x: f32,
    pub y: f32,
    pub preview: Option<NodeId>,
    /// Where the preview's top-left goes, where a host shows it in a window
    /// of its own (`set_drag_preview_origin`).
    pub preview_origin: Option<(f32, f32)>,
    pub export: Option<Rc<[DragItem]>>,
}
/// How a window's drag ended, for its host (a preview shown in a window of
/// its own slides back to its source when nothing took it).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DragEnded {
    pub source: NodeId,
    pub preview: Option<NodeId>,
    pub accepted: bool,
}
#[derive(Clone)]
pub(super) struct DragSession {
    pub(super) source: NodeId,
    payload: Action,
    x: f32,
    y: f32,
    grab: (f32, f32),
    destination: Option<NodeId>,
    preview: Option<NodeId>,
    preview_origin: Option<(f32, f32)>,
    export: Option<Rc<[DragItem]>>,
}
impl DragSession {
    fn event(&self, phase: DragPhase, accepted: bool) -> InputEvent {
        InputEvent::Drag(DragEvent {
            phase,
            payload: self.payload.clone(),
            source: self.source,
            x: self.x,
            y: self.y,
            grab_x: self.grab.0,
            grab_y: self.grab.1,
            accepted,
        })
    }
}
impl InputDispatcher {
    pub fn is_dragging(&self) -> bool {
        self.state.borrow().drag.is_some()
    }
    /// Exclude a retained preview subtree from drag hit testing.
    pub fn set_drag_preview(&self, node: Option<NodeId>) {
        if let Some(drag) = &mut self.state.borrow_mut().drag {
            drag.preview = node;
        }
    }
    /// Where the preview's top-left goes, in window coordinates, for a host
    /// showing it in a window of its own (it is then not moved in the scene).
    pub fn set_drag_preview_origin(&self, origin: Option<(f32, f32)>) {
        if let Some(drag) = &mut self.state.borrow_mut().drag {
            drag.preview_origin = origin;
        }
    }
    pub fn drag_state(&self) -> Option<DragState> {
        self.state.borrow().drag.as_ref().map(|drag| DragState {
            payload: drag.payload.clone(),
            source: drag.source,
            x: drag.x,
            y: drag.y,
            preview: drag.preview,
            preview_origin: drag.preview_origin,
            export: drag.export.clone(),
        })
    }
    /// How the last drag ended, once.
    pub fn take_drag_ended(&self) -> Option<DragEnded> {
        self.state.borrow_mut().drag_ended.take()
    }
    /// The drag ended where the window does not see it (handed to the
    /// platform, another app took it or none did): the source hears it end,
    /// `accepted` or not, and nothing here is dropped on.
    pub fn end_drag_elsewhere(&self, scene: &Rc<RefCell<Scene>>, accepted: bool) {
        self.finish(scene, false, accepted);
    }
    pub fn cancel_drag(&self, scene: &Rc<RefCell<Scene>>) {
        self.finish_drag(scene, false);
    }
    pub fn validate_drag(&self, scene: &Rc<RefCell<Scene>>) {
        let source = self.state.borrow().drag.as_ref().map(|drag| drag.source);
        if source.is_some_and(|source| !self.enabled(&scene.borrow(), source)) {
            self.cancel_drag(scene);
            return;
        }
        let drag = self.state.borrow().drag.clone();
        if let Some(drag) = drag
            && let Some(destination) = drag.destination
            && !self.enabled(&scene.borrow(), destination)
        {
            if let Some(active) = &mut self.state.borrow_mut().drag {
                active.destination = None;
            }
            if scene.borrow().contains(destination) {
                self.route(scene, destination, drag.event(DragPhase::Leave, false));
            }
        }
    }
    pub(super) fn begin_drag(
        &self,
        scene: &Rc<RefCell<Scene>>,
        source: NodeId,
        payload: Action,
        x: f32,
        y: f32,
        pressed_at: (f32, f32),
    ) {
        if self.is_dragging() || !self.enabled(&scene.borrow(), source) {
            return;
        }
        self.cancel_pending_keys();
        self.set_hover(scene, None);
        if !self.enabled(&scene.borrow(), source) {
            return;
        }
        let origin = scene.borrow().bounds(source);
        let drag = DragSession {
            source,
            payload,
            x,
            y,
            grab: (pressed_at.0 - origin.x, pressed_at.1 - origin.y),
            destination: None,
            preview: None,
            preview_origin: None,
            export: None,
        };
        let pressed = {
            let mut state = self.state.borrow_mut();
            state.drag = Some(drag.clone());
            state.drag_orphans.clear();
            let pressed = state.pressed.take();
            state.key_pressed = None;
            state.clicks.0 = None;
            pressed
        };
        if let Some(pressed) = pressed.filter(|id| *id != source && scene.borrow().contains(*id)) {
            self.route(scene, pressed, InputEvent::PointerCancel);
        }
        self.route(scene, source, InputEvent::PointerCancel);
        let started = self.route(scene, source, drag.event(DragPhase::Start, false));
        if let Some(items) = started.drag_export
            && let Some(active) = &mut self.state.borrow_mut().drag
        {
            active.export = Some(items);
        }
        self.update_drag(scene, x, y);
    }
    fn drag_hit(&self, scene: &Scene, x: f32, y: f32, preview: Option<NodeId>) -> Option<NodeId> {
        'hits: for hit in scene.hit_test_all(x, y) {
            if preview.is_some_and(|preview| {
                hit == preview || scene.ancestors(hit).any(|id| id == preview)
            }) {
                continue;
            }
            let mut node = Some(hit);
            while let Some(id) = node {
                // `pointer_events = "none"`: drops land on what is under it.
                if self.options(id).is_some_and(|options| options.pass_through) {
                    continue 'hits;
                }
                if !self.enabled(scene, id) {
                    break;
                }
                if self.has_listeners(id) {
                    return Some(id);
                }
                node = scene.parent(id);
            }
        }
        None
    }
    fn update_drag(&self, scene: &Rc<RefCell<Scene>>, x: f32, y: f32) {
        self.validate_drag(scene);
        let Some(mut drag) = self.state.borrow().drag.clone() else {
            return;
        };
        if !x.is_finite() || !y.is_finite() {
            return;
        }
        drag.x = x;
        drag.y = y;
        if let Some(active) = &mut self.state.borrow_mut().drag {
            active.x = x;
            active.y = y;
        }
        self.route(scene, drag.source, drag.event(DragPhase::Move, false));
        if !self
            .state
            .borrow()
            .drag
            .as_ref()
            .is_some_and(|active| active.source == drag.source && active.payload == drag.payload)
        {
            return;
        }
        let hit = self.drag_hit(&scene.borrow(), x, y, drag.preview);
        let destination = hit
            .and_then(|hit| {
                self.route(scene, hit, drag.event(DragPhase::Over, false))
                    .drag_accepted
            })
            .filter(|id| self.enabled(&scene.borrow(), *id));
        if drag.destination != destination
            && let Some(old) = drag.destination.filter(|id| scene.borrow().contains(*id))
        {
            self.route(scene, old, drag.event(DragPhase::Leave, false));
        }
        if let Some(active) = &mut self.state.borrow_mut().drag {
            active.destination = destination;
        }
    }
    fn finish_drag(&self, scene: &Rc<RefCell<Scene>>, drop: bool) {
        self.finish(scene, drop, false);
    }
    /// End the drag: dropped here (`drop`), or taken elsewhere (`elsewhere`).
    fn finish(&self, scene: &Rc<RefCell<Scene>>, drop: bool, elsewhere: bool) {
        let Some(drag) = self.state.borrow_mut().drag.take() else {
            return;
        };
        self.state.borrow_mut().drag_ending = Some(drag.source);
        {
            let mut state = self.state.borrow_mut();
            state.pressed = None;
            state.captured = None;
            state.capture_button = None;
        }
        let accepted = elsewhere
            || drop
                && drag
                    .destination
                    .filter(|id| self.enabled(&scene.borrow(), *id))
                    .is_some_and(|id| {
                        self.route(scene, id, drag.event(DragPhase::Drop, false))
                            .prevented
                    });
        self.state.borrow_mut().drag_ended = Some(DragEnded {
            source: drag.source,
            preview: drag.preview,
            accepted,
        });
        if let Some(id) = drag.destination.filter(|id| scene.borrow().contains(*id)) {
            self.route(scene, id, drag.event(DragPhase::Leave, accepted));
        }
        let orphans = {
            let mut state = self.state.borrow_mut();
            state.drag_ending = None;
            std::mem::take(&mut state.drag_orphans)
        };
        if scene.borrow().contains(drag.source) {
            self.route(scene, drag.source, drag.event(DragPhase::End, accepted));
        } else {
            // The source went during the drag: its own listeners still learn
            // that it ended (and whether it was taken), at the target phase.
            let mut cx = EventContext::new(drag.event(DragPhase::End, accepted), drag.source);
            for callback in orphans {
                if let Ok(mut callback) = callback.try_borrow_mut() {
                    callback(&mut cx);
                }
            }
        }
        if drop {
            let hovered = self.hit_target(&scene.borrow(), drag.x, drag.y);
            self.set_hover(scene, hovered);
        }
    }
    pub(super) fn dispatch_drag(&self, scene: &Rc<RefCell<Scene>>, event: &InputEvent) -> bool {
        self.validate_drag(scene);
        if !self.is_dragging() {
            return false;
        }
        match event {
            InputEvent::PointerMove { x, y } => {
                self.update_drag(scene, *x, *y);
                true
            }
            InputEvent::PointerUp {
                x,
                y,
                button: PointerButton::Primary,
            } => {
                self.update_drag(scene, *x, *y);
                self.finish_drag(scene, true);
                true
            }
            InputEvent::PointerCancel
            | InputEvent::Blur
            | InputEvent::KeyDown {
                key: Key::Escape, ..
            } => {
                self.cancel_drag(scene);
                true
            }
            InputEvent::PointerLeave => {
                let drag = self.state.borrow().drag.clone().unwrap();
                if let Some(id) = drag.destination.filter(|id| scene.borrow().contains(*id)) {
                    self.route(scene, id, drag.event(DragPhase::Leave, false));
                }
                if let Some(drag) = &mut self.state.borrow_mut().drag {
                    drag.destination = None;
                }
                true
            }
            _ => false,
        }
    }
}
