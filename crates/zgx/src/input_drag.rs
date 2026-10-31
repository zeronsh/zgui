//! One window's typed drag session, and drags from other windows over it.
//!
//! A host with several windows carries a drag between them: while the
//! pointer is over another window, the source's session is away
//! (`set_drag_away`) and that window's destinations hear it as a visit
//! (`drag_over`, `drag_out`, `drag_drop`); the source then learns how it
//! ended (`end_drag_elsewhere`). A host handing it to the platform as it
//! leaves the app's windows (to other apps) reads what it carries from
//! `drag_state`.
use super::*;
use crate::actions::Action;
/// Which point a drag between nodes is at (see [`DragEvent`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DragPhase {
    /// It began; sent to the source.
    Start,
    /// The pointer moved; sent to the source.
    Move,
    /// The pointer is over a node with listeners, which takes the drag as
    /// its destination by calling `accept_drag`.
    Over,
    /// The pointer left the destination, or the drag ended over it; sent to
    /// the destination.
    Leave,
    /// The primary button came up over the destination, which takes the
    /// drop by calling `prevent_default`.
    Drop,
    /// It ended, dropped or not; sent to the source.
    End,
    /// The pointer has rested on the destination for the spring-loading
    /// delay (`set_spring_delay`), or Space was pressed there: sent to the
    /// destination, which springs open (a tab shows, a folder opens) and
    /// says so with `prevent_default`. Once per destination: moving within
    /// it does not start the delay again; leaving it does.
    Spring,
}
/// How long a drag rests on a destination before it springs open, where the
/// host does not say (`InputDispatcher::set_spring_delay`): macOS's default,
/// and Chrome's for its tabs.
pub const SPRING_DELAY: std::time::Duration = std::time::Duration::from_millis(500);
/// The destination a drag rests on, from when, and whether it sprang.
#[derive(Clone, Copy)]
pub(super) struct SpringArm {
    node: NodeId,
    since: Instant,
    fired: bool,
}
/// A drag between nodes, at one of its phases.
#[derive(Clone, Debug, PartialEq)]
pub struct DragEvent {
    /// Which phase.
    pub phase: DragPhase,
    /// What it carries.
    pub payload: Action,
    /// The node it started from.
    pub source: NodeId,
    /// The pointer, across the window.
    pub x: f32,
    /// The pointer, down the window.
    pub y: f32,
    /// The press's distance from the source's left edge. Subtracting it and
    /// `grab_y` from `x`/`y` keeps a preview under the original grab point.
    pub grab_x: f32,
    /// The press's distance from the source's top edge (see `grab_x`).
    pub grab_y: f32,
    /// Whether something took the drop: on `End`, and on the destination's
    /// final `Leave`. A destination here consumed `Drop`, or another app took
    /// it.
    pub accepted: bool,
    /// On End: released outside every window of the app (where its host
    /// can tell), as a tab torn off its window is.
    pub outside: bool,
    /// From another window of the app (`InputDispatcher::drag_over`):
    /// `source` and the grab point are not this window's.
    pub visiting: bool,
}
/// Text dragged out of an editor or selectable text, as AppKit's text views
/// and browsers drag a selection: an editor takes it where it is dropped (a
/// view accepts it with `View::on_drop::<DraggedText>`), and other apps take
/// it as plain text.
#[derive(Debug)]
pub struct DraggedText {
    text: String,
    /// The editor it was selected in (its scene's `identity`, as node ids
    /// are each window's own, and its node), and where: a drop there moves
    /// it.
    pub(crate) from: Option<(u64, NodeId, std::ops::Range<usize>)>,
    /// A destination took it to keep (moved, not copied): its source lets
    /// it go as the drag ends.
    pub(crate) taken: std::cell::Cell<bool>,
}
impl DraggedText {
    /// Dragged text that no editor gives up: copied wherever it is dropped.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            from: None,
            taken: std::cell::Cell::new(false),
        }
    }
    pub(crate) fn from_editor(
        text: String,
        scene: u64,
        editor: NodeId,
        range: std::ops::Range<usize>,
    ) -> Self {
        Self {
            text,
            from: Some((scene, editor, range)),
            taken: std::cell::Cell::new(false),
        }
    }
    /// Where in editor `editor` of the scene `scene` (its `identity`) it was
    /// selected, if there.
    pub(crate) fn taken_from(&self, scene: u64, editor: NodeId) -> Option<&std::ops::Range<usize>> {
        self.from
            .as_ref()
            .filter(|(of, node, _)| *of == scene && *node == editor)
            .map(|(.., range)| range)
    }
    /// The text.
    pub fn text(&self) -> &str {
        &self.text
    }
}
/// What a drag carries to other apps, when a host hands it to the platform
/// as it leaves the window (`View::drag_export`).
#[derive(Clone, Debug, PartialEq)]
pub enum DragItem {
    /// Plain text.
    Text(String),
    /// A URL.
    Url(String),
    /// A file, by its path.
    File(std::path::PathBuf),
    /// An encoded image (PNG, JPEG, TIFF).
    Image(Rc<[u8]>),
}
/// A window's drag, as its host reads it (`InputDispatcher::drag_state`).
#[derive(Clone, Debug)]
pub struct DragState {
    /// What it carries.
    pub payload: Action,
    /// The node it started from.
    pub source: NodeId,
    /// The pointer, in the window's coordinates.
    pub x: f32,
    /// The pointer's y, in the window's coordinates.
    pub y: f32,
    /// The node shown as its preview, if one is set (`set_drag_preview`).
    pub preview: Option<NodeId>,
    /// Where the preview's top-left goes, where a host shows it in a window
    /// of its own (`set_drag_preview_origin`).
    pub preview_origin: Option<(f32, f32)>,
    /// What it carries to other apps, if its source set any as it started
    /// (`View::drag_export`).
    pub export: Option<Rc<[DragItem]>>,
    /// Over another window (`set_drag_away`).
    pub away: bool,
    /// It moves its window, the grab point under the pointer, rather than
    /// showing a preview (`View::drag_moves_window`): a window's only tab.
    pub moves_window: bool,
}
/// How a window's drag ended, for its host (a preview shown in a window of
/// its own slides back to its source when nothing took it).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DragEnded {
    /// The node it started from.
    pub source: NodeId,
    /// Its preview node, if it had one.
    pub preview: Option<NodeId>,
    /// Whether something took it: a destination here took the drop, or
    /// the host reported another app did (`end_drag_elsewhere`).
    pub accepted: bool,
    /// Released outside the app's windows and taken there by its source
    /// (`View::on_drag_outside`: a tab torn off into a window of its own).
    pub outside: bool,
}
/// A visit: another window's drag over this one.
#[derive(Clone)]
pub(super) struct Visit {
    payload: Action,
    x: f32,
    y: f32,
    destination: Option<NodeId>,
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
    away: bool,
    moves_window: bool,
}
impl Visit {
    /// The node it names.
    pub(super) fn node(&self) -> Option<NodeId> {
        self.destination
    }
}
impl DragSession {
    /// The nodes it names.
    pub(super) fn nodes(&self) -> impl Iterator<Item = NodeId> {
        [Some(self.source), self.destination, self.preview]
            .into_iter()
            .flatten()
    }
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
            outside: false,
            visiting: false,
        })
    }
}
impl Visit {
    fn event(&self, phase: DragPhase, root: NodeId) -> InputEvent {
        InputEvent::Drag(DragEvent {
            phase,
            payload: self.payload.clone(),
            source: root,
            x: self.x,
            y: self.y,
            grab_x: 0.,
            grab_y: 0.,
            accepted: false,
            outside: false,
            visiting: true,
        })
    }
}
impl InputDispatcher {
    /// Whether a drag is under way.
    pub fn is_dragging(&self) -> bool {
        self.state.borrow().drag.is_some()
    }
    /// Whether a drag under way moves its window (`View::drag_moves_window`).
    pub fn drag_moves_window(&self) -> bool {
        self.state
            .borrow()
            .drag
            .as_ref()
            .is_some_and(|drag| drag.moves_window)
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
    /// The drag under way, if any.
    pub fn drag_state(&self) -> Option<DragState> {
        self.state.borrow().drag.as_ref().map(|drag| DragState {
            payload: drag.payload.clone(),
            source: drag.source,
            x: drag.x,
            y: drag.y,
            preview: drag.preview,
            preview_origin: drag.preview_origin,
            export: drag.export.clone(),
            away: drag.away,
            moves_window: drag.moves_window,
        })
    }
    /// How the last drag ended, once.
    pub fn take_drag_ended(&self) -> Option<DragEnded> {
        self.state.borrow_mut().drag_ended.take()
    }
    /// The drag ended where the window does not see it: over another of the
    /// app's windows, handed to the platform (another app took it or none
    /// did), or, `outside`, released over none of the app's windows. The
    /// source hears it end, `accepted` or not, and nothing here is dropped
    /// on.
    pub fn end_drag_elsewhere(&self, scene: &Rc<RefCell<Scene>>, accepted: bool, outside: bool) {
        self.finish(scene, false, accepted, outside);
    }
    /// The pointer is over another window (or back over this one): no
    /// destination here hears the drag meanwhile; the source still hears it
    /// move.
    pub fn set_drag_away(&self, scene: &Rc<RefCell<Scene>>, away: bool) {
        let drag = match &self.state.borrow().drag {
            Some(drag) if drag.away != away => drag.clone(),
            _ => return,
        };
        if let Some(active) = &mut self.state.borrow_mut().drag {
            active.away = away;
            if away {
                active.destination = None;
            }
        }
        if away {
            // Back over a destination here, its delay starts again.
            self.arm_spring(None);
        }
        if away && let Some(id) = drag.destination.filter(|id| scene.borrow().contains(*id)) {
            self.route(scene, id, drag.event(DragPhase::Leave, false));
        }
    }
    /// Another window's drag carrying `payload` is at window point `(x, y)`
    /// here: destinations here hear it over them (`DragEvent::visiting`).
    /// Returns whether one accepts it.
    pub fn drag_over(&self, scene: &Rc<RefCell<Scene>>, payload: &Action, x: f32, y: f32) -> bool {
        if !x.is_finite() || !y.is_finite() {
            return false;
        }
        let previous = {
            let mut state = self.state.borrow_mut();
            let visit = state.visit.get_or_insert_with(|| Visit {
                payload: payload.clone(),
                x,
                y,
                destination: None,
            });
            // What it carries can be known only as it drops (another app's
            // text: X11 hands it over then): where it was over hears it.
            if visit.payload != *payload {
                visit.payload = payload.clone();
            }
            visit.x = x;
            visit.y = y;
            visit.clone()
        };
        let root = scene.borrow().root();
        // Back over its own window, the drag's preview is not under it.
        let preview = self
            .state
            .borrow()
            .drag
            .as_ref()
            .and_then(|drag| drag.preview);
        let hit = self.drag_hit(&scene.borrow(), x, y, preview);
        let destination = hit
            .and_then(|hit| {
                self.route(scene, hit, previous.event(DragPhase::Over, root))
                    .drag_accepted
            })
            .filter(|id| self.enabled(&scene.borrow(), *id));
        if previous.destination != destination
            && let Some(old) = previous
                .destination
                .filter(|id| scene.borrow().contains(*id))
        {
            self.route(scene, old, previous.event(DragPhase::Leave, root));
        }
        if let Some(visit) = &mut self.state.borrow_mut().visit {
            visit.destination = destination;
        }
        destination.is_some()
    }
    /// Another window's drag left this one.
    pub fn drag_out(&self, scene: &Rc<RefCell<Scene>>) {
        let Some(visit) = self.state.borrow_mut().visit.take() else {
            return;
        };
        let root = scene.borrow().root();
        if let Some(id) = visit.destination.filter(|id| scene.borrow().contains(*id)) {
            self.route(scene, id, visit.event(DragPhase::Leave, root));
        }
    }
    /// Another window's drag was released at window point `(x, y)` here:
    /// whether a destination here took it.
    pub fn drag_drop(&self, scene: &Rc<RefCell<Scene>>, payload: &Action, x: f32, y: f32) -> bool {
        self.drag_over(scene, payload, x, y);
        let Some(visit) = self.state.borrow_mut().visit.take() else {
            return false;
        };
        let root = scene.borrow().root();
        let (accepted, focus) = visit
            .destination
            .filter(|id| self.enabled(&scene.borrow(), *id))
            .map_or((false, None), |id| {
                let cx = self.route(scene, id, visit.event(DragPhase::Drop, root));
                (cx.prevented, cx.focus)
            });
        if let Some(id) = visit.destination.filter(|id| scene.borrow().contains(*id)) {
            self.route(scene, id, visit.event(DragPhase::Leave, root));
        }
        self.focus_dropped(scene, focus);
        if accepted {
            let hovered = self.hit_target(&scene.borrow(), x, y);
            self.set_hover(scene, hovered);
        }
        accepted
    }
    /// A destination taking a drop asked for focus (an editor, as AppKit's
    /// text views and a browser's fields): it has it once the drop is over.
    fn focus_dropped(&self, scene: &Rc<RefCell<Scene>>, focus: Option<NodeId>) {
        if let Some(focus) = focus.filter(|id| self.enabled(&scene.borrow(), *id)) {
            self.focus(scene, Some(focus));
        }
    }
    /// End the drag without a drop: the destination hears `Leave`, and the
    /// source `End`, not accepted.
    pub fn cancel_drag(&self, scene: &Rc<RefCell<Scene>>) {
        self.finish_drag(scene, false);
    }
    /// Cancel the drag if its source shows but can no longer take input
    /// (disabled, or outside the modal scope), and let go of a destination
    /// that can't, which hears `Leave`. A source hidden or removed keeps
    /// its drag, as a browser's and AppKit's do: what it carries may be
    /// dropped where the drag itself led (a pane held over a tab, whose
    /// tab then shows, its own hidden), and its listeners still hear the
    /// drag move and end.
    pub fn validate_drag(&self, scene: &Rc<RefCell<Scene>>) {
        let source = self.state.borrow().drag.as_ref().map(|drag| drag.source);
        if source.is_some_and(|source| {
            let scene = scene.borrow();
            scene.layout_visible(source) && !self.enabled(&scene, source)
        }) {
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
            self.arm_spring(None);
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
        // From where it shows: a preview goes in the window's points.
        let origin = scene.borrow().transformed_bounds(source);
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
            away: false,
            moves_window: false,
        };
        let (pressed, orphans) = {
            let mut state = self.state.borrow_mut();
            state.drag = Some(drag.clone());
            state.spring = None;
            let pressed = state.pressed.take();
            state.key_pressed = None;
            state.clicks.0 = None;
            (pressed, std::mem::take(&mut state.drag_orphans))
        };
        // A last drag's orphaned listeners, out of the borrow (see `finish`).
        drop(orphans);
        if let Some(pressed) = pressed.filter(|id| *id != source && scene.borrow().contains(*id)) {
            self.route(scene, pressed, InputEvent::PointerCancel);
        }
        self.route(scene, source, InputEvent::PointerCancel);
        let started = self.route(scene, source, drag.event(DragPhase::Start, false));
        if let Some(active) = &mut self.state.borrow_mut().drag {
            active.export = started.drag_export;
            active.moves_window = started.drag_moves_window;
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
                if self
                    .options(id)
                    .is_some_and(|options| self.passes(scene, id, &options))
                {
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
        self.to_source(scene, drag.source, drag.event(DragPhase::Move, false));
        // Over another window, its destinations hear it (`drag_over`).
        if drag.away {
            return;
        }
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
        self.arm_spring(destination);
    }
    /// The spring-loading delay starts as a drag comes to rest on a new
    /// destination (or stops, on none); within the same one it runs on.
    fn arm_spring(&self, destination: Option<NodeId>) {
        let mut state = self.state.borrow_mut();
        if state.spring.map(|arm| arm.node) != destination {
            state.spring = destination.map(|node| SpringArm {
                node,
                since: Instant::now(),
                fired: false,
            });
        }
    }
    /// How long a drag rests on a destination before it springs open
    /// (`DragPhase::Spring`): the platform's setting, which the host reads
    /// (macOS's spring-loading delay), or `None` where it is turned off.
    /// [`SPRING_DELAY`] until set.
    pub fn set_spring_delay(&self, delay: Option<std::time::Duration>) {
        self.state.borrow_mut().spring_delay = Some(delay);
    }
    fn spring_delay(&self) -> Option<std::time::Duration> {
        self.state
            .borrow()
            .spring_delay
            .unwrap_or(Some(SPRING_DELAY))
    }
    /// When the destination a drag rests on springs open, if one is waiting.
    pub fn next_spring_deadline(&self) -> Option<Instant> {
        let delay = self.spring_delay()?;
        let state = self.state.borrow();
        let destination = state.drag.as_ref()?.destination;
        // Only for the destination it rests on now: nothing stale stays due.
        state
            .spring
            .filter(|arm| !arm.fired && Some(arm.node) == destination)
            .map(|arm| arm.since + delay)
    }
    /// Spring open the destination the drag has rested on long enough by
    /// `now`; whether one heard it.
    pub fn advance_spring(&self, scene: &Rc<RefCell<Scene>>, now: Instant) -> bool {
        match self.next_spring_deadline() {
            Some(at) if at <= now => self.spring_now(scene).is_some(),
            _ => false,
        }
    }
    /// Spring open the destination under the drag now: `None` if there is
    /// none waiting, else whether it took it (`prevent_default`).
    fn spring_now(&self, scene: &Rc<RefCell<Scene>>) -> Option<bool> {
        self.spring_delay()?;
        let drag = self.state.borrow().drag.clone()?;
        let node = {
            let mut state = self.state.borrow_mut();
            let arm = state.spring.as_mut()?;
            if arm.fired || Some(arm.node) != drag.destination {
                return None;
            }
            arm.fired = true;
            arm.node
        };
        if !self.enabled(&scene.borrow(), node) {
            return None;
        }
        Some(
            self.route(scene, node, drag.event(DragPhase::Spring, false))
                .prevented,
        )
    }
    fn finish_drag(&self, scene: &Rc<RefCell<Scene>>, drop: bool) {
        self.finish(scene, drop, false, false);
    }
    /// End the drag: dropped here (`drop`), or ended elsewhere, taken there
    /// (`elsewhere`) or released `outside` the app's windows.
    fn finish(&self, scene: &Rc<RefCell<Scene>>, drop: bool, elsewhere: bool, outside: bool) {
        let Some(drag) = self.state.borrow_mut().drag.take() else {
            return;
        };
        self.state.borrow_mut().spring = None;
        self.state.borrow_mut().drag_ending = Some(drag.source);
        {
            let mut state = self.state.borrow_mut();
            state.pressed = None;
            state.captured = None;
            state.capture_button = None;
        }
        let (dropped, focus) = drag
            .destination
            .filter(|id| drop && !elsewhere && self.enabled(&scene.borrow(), *id))
            .map_or((false, None), |id| {
                let cx = self.route(scene, id, drag.event(DragPhase::Drop, false));
                (cx.prevented, cx.focus)
            });
        let accepted = elsewhere || dropped;
        if let Some(id) = drag.destination.filter(|id| scene.borrow().contains(*id)) {
            self.route(scene, id, drag.event(DragPhase::Leave, accepted));
        }
        self.focus_dropped(scene, focus);
        let mut end = drag.event(DragPhase::End, accepted);
        if let InputEvent::Drag(event) = &mut end {
            event.outside = outside;
        }
        let torn = self.to_source(scene, drag.source, end).prevented;
        // The source's orphaned listeners go out of the borrow: what they
        // hold (a preview's view, say) may reach input as it drops.
        let orphans = {
            let mut state = self.state.borrow_mut();
            state.drag_ending = None;
            std::mem::take(&mut state.drag_orphans)
        };
        std::mem::drop(orphans);
        self.state.borrow_mut().drag_ended = Some(DragEnded {
            source: drag.source,
            preview: drag.preview,
            accepted,
            outside: outside && torn,
        });
        if drop {
            let hovered = self.hit_target(&scene.borrow(), drag.x, drag.y);
            self.set_hover(scene, hovered);
        }
    }
    /// `event` to the drag's source; or, the source gone during the drag,
    /// to its own listeners, which still learn how it goes (and whether it
    /// was taken), at the target phase.
    fn to_source(
        &self,
        scene: &Rc<RefCell<Scene>>,
        source: NodeId,
        event: InputEvent,
    ) -> EventContext {
        if scene.borrow().contains(source) {
            return self.route(scene, source, event);
        }
        let orphans = self.state.borrow().drag_orphans.clone();
        let mut cx = EventContext::new(event, source);
        for callback in orphans {
            if let Ok(mut callback) = callback.try_borrow_mut() {
                callback(&mut cx);
            }
        }
        cx
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
            // Space springs the destination open at once, as in Finder
            // (taken only if it did: otherwise the key goes on as usual).
            InputEvent::KeyDown {
                key: Key::Space, ..
            } if self.spring_now(scene) == Some(true) => true,
            InputEvent::PointerLeave => {
                let drag = self.state.borrow().drag.clone().unwrap();
                if let Some(id) = drag.destination.filter(|id| scene.borrow().contains(*id)) {
                    self.route(scene, id, drag.event(DragPhase::Leave, false));
                }
                if let Some(drag) = &mut self.state.borrow_mut().drag {
                    drag.destination = None;
                }
                self.arm_spring(None);
                true
            }
            _ => false,
        }
    }
}
