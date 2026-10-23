//! Retained event routing. Bindings disconnect when dropped; callbacks run without a scene borrow.
use crate::{
    reactive::{Runtime, Signal},
    scene::{NodeId, Scene},
};
use std::{
    cell::RefCell,
    collections::HashMap,
    rc::{Rc, Weak},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
    pub meta: bool,
}
impl Modifiers {
    /// Command on macOS, Control on Linux and other desktop platforms.
    pub fn primary_shortcut(self) -> bool {
        if cfg!(target_os = "macos") {
            self.meta
        } else {
            self.control
        }
    }
    /// Option moves by words on macOS; Control does so on Linux.
    pub fn word_navigation(self) -> bool {
        if cfg!(target_os = "macos") {
            self.alt
        } else {
            self.control
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Key {
    Tab,
    Enter,
    Space,
    Escape,
    Backspace,
    Delete,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowDown,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    /// Function key number (native host supplies 1–35).
    Function(u8),
    Character(String),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerButton {
    Primary,
    Secondary,
    Middle,
    Other(u16),
}
/// Complete native file transfers are rejected instead of truncated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileDropError {
    TooManyFiles,
    TooLarge,
    InvalidData,
    TimedOut,
}
#[derive(Clone, Debug, PartialEq)]
pub enum InputEvent {
    Action(crate::actions::Action),
    Drag(DragEvent),
    FileHover {
        x: f32,
        y: f32,
        path: std::path::PathBuf,
    },
    FileDrop {
        x: f32,
        y: f32,
        path: std::path::PathBuf,
    },
    FileHoverCancelled,
    FilesDropRejected {
        x: f32,
        y: f32,
        reason: FileDropError,
    },
    /// Paths delivered together by one native event-loop dispatch batch.
    FilesDrop {
        x: f32,
        y: f32,
        paths: std::sync::Arc<[std::path::PathBuf]>,
    },
    PointerDown {
        x: f32,
        y: f32,
        button: PointerButton,
    },
    PointerUp {
        x: f32,
        y: f32,
        button: PointerButton,
    },
    PointerMove {
        x: f32,
        y: f32,
    },
    PointerLeave,
    PointerCancel,
    PointerEnter,
    Scroll {
        x: f32,
        y: f32,
        delta_x: f32,
        delta_y: f32,
    },
    KeyDown {
        key: Key,
        modifiers: Modifiers,
        repeat: bool,
    },
    KeyUp {
        key: Key,
        modifiers: Modifiers,
    },
    Text(String),
    ImePreedit {
        text: String,
        cursor: Option<(usize, usize)>,
    },
    ImeCommit(String),
    Focus,
    FocusScopeClosed,
    Blur,
    Activate,
    SetValue(String),
    /// Set committed-text selection using UTF-8 byte offsets.
    SetTextSelection {
        anchor: usize,
        focus: usize,
    },
    SetNumericValue(f64),
    Increment,
    Decrement,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventPhase {
    Capture,
    Target,
    Bubble,
}
/// Source, payload and optional window press position of a requested drag.
type DragRequest = (NodeId, crate::actions::Action, Option<(f32, f32)>);
pub struct EventContext {
    pub event: InputEvent,
    pub target: NodeId,
    pub current_target: NodeId,
    pub phase: EventPhase,
    stopped: bool,
    immediate: bool,
    prevented: bool,
    focus: Option<NodeId>,
    capture: Option<Option<NodeId>>,
    pointer_modifiers: Modifiers,
    click_count: u8,
    drag_request: Option<DragRequest>,
    drag_accepted: Option<NodeId>,
    key_code: Option<Rc<str>>,
    /// `take_pointer`'s taker: what the pointer was pressed on gives it up.
    take_pointer: Option<NodeId>,
}
impl EventContext {
    /// `event` at `target`, in the target phase, nothing decided yet.
    pub(crate) fn new(event: InputEvent, target: NodeId) -> Self {
        Self {
            event,
            target,
            current_target: target,
            phase: EventPhase::Target,
            stopped: false,
            immediate: false,
            prevented: false,
            focus: None,
            capture: None,
            pointer_modifiers: Modifiers::default(),
            click_count: 0,
            drag_request: None,
            drag_accepted: None,
            key_code: None,
            take_pointer: None,
        }
    }
    /// Request a typed drag from this listener during a pointer move.
    /// Start a drag from this pointer move; the current position is the grab point.
    pub fn start_drag<T: 'static>(&mut self, payload: T) {
        self.request_drag(payload, None);
    }
    /// Start a drag whose grab point is the earlier press at window `x`/`y`.
    pub fn start_drag_from<T: 'static>(&mut self, payload: T, x: f32, y: f32) {
        self.request_drag(payload, Some((x, y)));
    }
    fn request_drag<T: 'static>(&mut self, payload: T, pressed_at: Option<(f32, f32)>) {
        if matches!(self.event, InputEvent::PointerMove { .. }) && self.drag_request.is_none() {
            self.drag_request = Some((
                self.current_target,
                crate::actions::Action::new(payload),
                pressed_at,
            ));
            self.prevent_default();
        }
    }
    pub fn accept_drag(&mut self) {
        if matches!(&self.event,InputEvent::Drag(event) if event.phase==DragPhase::Over)
            && self.phase != EventPhase::Capture
            && self.drag_accepted.is_none()
        {
            self.drag_accepted = Some(self.current_target);
        }
    }

    /// Modifier snapshot for this routed pointer event.
    pub fn pointer_modifiers(&self) -> Modifiers {
        self.pointer_modifiers
    }
    /// Primary or other button press count (1–3); zero for non-press events.
    pub fn click_count(&self) -> u8 {
        self.click_count
    }
    /// A key event's physical key, as the DOM's `KeyboardEvent.code`
    /// (`"KeyA"`, `"Digit1"`, `"BracketLeft"`): the same key whatever the
    /// layout or modifiers made it type. None when the platform didn't say.
    pub fn key_code(&self) -> Option<&str> {
        self.key_code.as_deref()
    }
    pub fn stop_propagation(&mut self) {
        self.stopped = true;
    }
    /// Stop remaining listeners on this node as well as propagation to ancestors.
    pub fn stop_immediate_propagation(&mut self) {
        self.stopped = true;
        self.immediate = true;
    }
    pub fn prevent_default(&mut self) {
        self.prevented = true;
    }
    pub fn focus(&mut self) {
        self.focus = Some(self.current_target);
    }
    /// Capture pointer routing until the initiating button is released or the
    /// gesture is cancelled. Captures outside pointer-down retain the current
    /// capture button, defaulting to the primary button when none exists.
    pub(crate) fn capture_requested(&self) -> bool {
        self.capture.is_some()
    }
    /// What a listener so far asked capture for: `Some(None)` a release.
    pub(crate) fn capture_target(&self) -> Option<Option<NodeId>> {
        self.capture
    }
    pub fn capture_pointer(&mut self) {
        self.capture = Some(Some(self.current_target));
    }
    pub fn release_pointer(&mut self) {
        self.capture = Some(None);
    }
    /// Capture the pointer from what it was pressed on, which hears a
    /// cancel and never activates: a drag that began on a link selects
    /// text instead. For pointer moves while the press is down.
    pub(crate) fn take_pointer(&mut self) {
        self.capture = Some(Some(self.current_target));
        self.take_pointer = Some(self.current_target);
    }
    pub fn default_prevented(&self) -> bool {
        self.prevented
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct NodeInput {
    pub focusable: bool,
    /// CSS `pointer-events: none`: the pointer passes through the node, and
    /// through descendants that have no listeners of their own, to what is
    /// under it.
    pub pass_through: bool,
    pub disabled: bool,
    pub tab_index: i32,
}
type Callback = Rc<RefCell<dyn FnMut(&mut EventContext)>>;
struct Entry {
    token: u64,
    callback: Callback,
}
struct FocusScope {
    root: NodeId,
    previous: Option<NodeId>,
    /// What the scope below holds stays live too (a submenu over its menu).
    porous: bool,
}
#[derive(Clone, Copy)]
struct Click {
    target: NodeId,
    button: PointerButton,
    position: (f32, f32),
    time: Instant,
    count: u8,
}
#[derive(Default)]
struct ClickTracker(Option<Click>);
impl ClickTracker {
    fn moved(&mut self, x: f32, y: f32) {
        if self
            .0
            .is_some_and(|click| !Self::near(click.position, (x, y)))
        {
            self.0 = None;
        }
    }
    fn near(a: (f32, f32), b: (f32, f32)) -> bool {
        let dx = f64::from(a.0) - f64::from(b.0);
        let dy = f64::from(a.1) - f64::from(b.1);
        dx * dx + dy * dy <= 16.
    }
    fn press(
        &mut self,
        target: NodeId,
        button: PointerButton,
        position: (f32, f32),
        now: Instant,
    ) -> u8 {
        let count = self
            .0
            .filter(|old| {
                old.target == target
                    && old.button == button
                    && now
                        .checked_duration_since(old.time)
                        .is_some_and(|elapsed| elapsed <= Duration::from_millis(500))
                    && Self::near(old.position, position)
            })
            .map_or(1, |old| old.count % 3 + 1);
        self.0 = Some(Click {
            target,
            button,
            position,
            time: now,
            count,
        });
        count
    }
}
#[derive(Default)]
struct State {
    entries: HashMap<NodeId, Vec<Entry>>,
    options: HashMap<NodeId, NodeInput>,
    disabled_observers: HashMap<NodeId, Signal<bool>>,
    next: u64,
    focused: Option<NodeId>,
    captured: Option<NodeId>,
    capture_button: Option<PointerButton>,
    /// The hovered node (the pointer's hit target) and its ancestors, up to
    /// the active modal root: each has had its enter, none its leave yet.
    hover: Vec<NodeId>,
    /// Where the pointer last was over the window (None: outside it).
    pointer: Option<(f32, f32)>,
    file_hovered: Option<NodeId>,
    pressed: Option<NodeId>,
    key_pressed: Option<(NodeId, Key)>,
    scopes: Vec<FocusScope>,
    /// Non-modal layers (toasts): what they hold stays live under any modal
    /// scope (`set_layer`).
    layers: Vec<NodeId>,
    clicks: ClickTracker,
    drag: Option<drag::DragSession>,
    /// Listeners of the drag's source removed while it went on (a drop that
    /// moves the dragged thing elsewhere): they still hear the drag end.
    drag_orphans: Vec<Callback>,
    /// The source of a drag being finished (its drop and end delivering).
    drag_ending: Option<NodeId>,
    /// The physical key of the key event being dispatched (`set_key_code`).
    key_code: Option<Rc<str>>,
    /// The modifier keys last known held (from the platform's pointer and
    /// key events): what an activation (a click) was made with.
    modifiers: Modifiers,
    /// The last interaction was the pointer (a press), not the keyboard:
    /// what it focuses shows no focus ring (`focus_visible_now`).
    pointer_modality: bool,
}
#[derive(Clone, Default)]
pub struct InputDispatcher {
    state: Rc<RefCell<State>>,
    actions: Rc<RefCell<crate::actions::Registry>>,
}
#[must_use = "Keep the binding alive while the node handles input"]
pub struct InputBinding {
    state: Weak<RefCell<State>>,
    node: NodeId,
    token: u64,
}
impl Drop for InputBinding {
    fn drop(&mut self) {
        if let Some(state) = self.state.upgrade() {
            let mut s = state.borrow_mut();
            let removed = s.entries.get_mut(&self.node).and_then(|listeners| {
                let index = listeners
                    .iter()
                    .position(|entry| entry.token == self.token)?;
                Some(listeners.remove(index))
            });
            if let Some(entry) = &removed
                && s.drag.as_ref().map(|drag| drag.source).or(s.drag_ending) == Some(self.node)
            {
                s.drag_orphans.push(entry.callback.clone());
            }
            let mut disabled_observer = None;
            if s.entries.get(&self.node).is_some_and(Vec::is_empty) {
                s.entries.remove(&self.node);
                s.options.remove(&self.node);
                disabled_observer = s.disabled_observers.get(&self.node).cloned();
                if s.focused == Some(self.node) {
                    s.focused = None;
                }
                if s.captured == Some(self.node) {
                    s.captured = None;
                    s.capture_button = None;
                }
                if s.pressed == Some(self.node) {
                    s.pressed = None;
                }
                if s.key_pressed
                    .as_ref()
                    .is_some_and(|(id, _)| *id == self.node)
                {
                    s.key_pressed = None;
                }
            }
            // Callback captures may own bindings or other resources whose destructors
            // reenter this dispatcher. Finish detaching before running user Drop code.
            drop(s);
            if let Some(observer) = disabled_observer {
                observer.set(false);
            }
            drop(removed);
        }
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct DispatchResult {
    pub target: Option<NodeId>,
    pub default_prevented: bool,
    pub focus_changed: bool,
}
impl InputDispatcher {
    pub fn new() -> Self {
        Self::default()
    }
    /// Register a scoped keymap. Keep its guard alive for the component lifetime.
    pub fn bind_keys(
        &self,
        node: NodeId,
        map: crate::actions::Keymap,
    ) -> crate::actions::KeymapBinding {
        crate::actions::Registry::bind(&self.actions, node, map)
    }
    /// Route a typed action from focus through its enabled ancestors.
    pub fn dispatch_action(
        &self,
        scene: &Rc<RefCell<Scene>>,
        action: crate::actions::Action,
    ) -> DispatchResult {
        let target = self
            .focused()
            .or_else(|| self.focus_scope())
            .unwrap_or_else(|| scene.borrow().root());
        self.dispatch_to(scene, target, InputEvent::Action(action))
    }
    /// A single wake deadline exists only while a key sequence is ambiguous.
    pub fn next_key_deadline(&self) -> Option<Instant> {
        self.actions.borrow().deadline()
    }
    /// Whether what takes focus now shows it (CSS's `:focus-visible`): not
    /// when the last interaction was a pointer press (a clicked button),
    /// yes after the keyboard, and at first.
    pub fn focus_visible_now(&self) -> bool {
        !self.state.borrow().pointer_modality
    }
    /// The physical key (a DOM `KeyboardEvent.code`) of the key events
    /// dispatched until it's set again: a platform sets it around each.
    pub fn set_key_code(&self, code: Option<Rc<str>>) {
        self.state.borrow_mut().key_code = code;
    }
    pub fn has_pending_keys(&self) -> bool {
        self.next_key_deadline().is_some()
    }
    pub fn cancel_pending_keys(&self) {
        self.actions.borrow_mut().cancel();
    }
    /// Resolve expired sequence candidates or replay their original input. The
    /// host calls this from its demand-driven interaction timer.
    pub fn advance_key_sequence(&self, scene: &Rc<RefCell<Scene>>, now: Instant) -> bool {
        let expired = self.actions.borrow_mut().expired(now);
        if let Some(replay) = expired {
            self.replay_keys(scene, replay);
            true
        } else {
            false
        }
    }
    fn replay_keys(
        &self,
        scene: &Rc<RefCell<Scene>>,
        replay: crate::actions::Replay,
    ) -> DispatchResult {
        let mut result = DispatchResult::default();
        if self.action_path(&scene.borrow()) != replay.path {
            return result;
        }
        for action in replay.candidates {
            if action.downcast_ref::<crate::actions::NoAction>().is_some() {
                return DispatchResult {
                    target: self.focused(),
                    default_prevented: true,
                    focus_changed: false,
                };
            }
            result = self.dispatch_action(scene, action);
            if result.default_prevented {
                return result;
            }
            if self.action_path(&scene.borrow()) != replay.path {
                result.default_prevented = true;
                return result;
            }
        }
        let mut suppress_text = false;
        for event in replay.events {
            if self.action_path(&scene.borrow()) != replay.path {
                result.default_prevented = true;
                break;
            }
            if suppress_text && matches!(event, InputEvent::Text(_)) {
                continue;
            }
            let key_down = matches!(event, InputEvent::KeyDown { .. });
            result = self.dispatch_impl(scene, event, None, false);
            if key_down {
                suppress_text = result.default_prevented;
            }
        }
        result
    }
    fn action_path(&self, scene: &Scene) -> Vec<NodeId> {
        let mut path = Vec::new();
        let mut node = self
            .focused()
            .or_else(|| self.focus_scope())
            .or(Some(scene.root()));
        while let Some(id) = node {
            if !self.enabled(scene, id) {
                break;
            }
            path.push(id);
            if Some(id) == self.focus_scope() {
                break;
            }
            node = scene.parent(id);
        }
        path
    }
    /// Replace this node's registrations. Use `listen` to extend existing behavior.
    pub fn register(
        &self,
        node: NodeId,
        options: NodeInput,
        callback: impl FnMut(&mut EventContext) + 'static,
    ) -> InputBinding {
        let mut s = self.state.borrow_mut();
        s.next = s.next.checked_add(1).expect("input binding IDs exhausted");
        let token = s.next;
        s.options.insert(node, options);
        if options.disabled {
            s.clicks.0 = None;
        }
        let disabled_observer = s.disabled_observers.get(&node).cloned();
        let replaced = s.entries.insert(
            node,
            vec![Entry {
                token,
                callback: Rc::new(RefCell::new(callback)),
            }],
        );
        drop(s);
        if let Some(observer) = disabled_observer {
            observer.set(options.disabled);
        }
        drop(replaced);
        InputBinding {
            state: Rc::downgrade(&self.state),
            node,
            token,
        }
    }
    /// Append a listener without replacing existing widget behavior or node options.
    /// Listeners run in registration order within each capture/target/bubble phase.
    pub fn listen(
        &self,
        node: NodeId,
        callback: impl FnMut(&mut EventContext) + 'static,
    ) -> InputBinding {
        self.listen_ordered(node, callback, false)
    }
    /// Prepend a listener while preserving existing registrations and node options.
    /// It runs before existing listeners in each phase; repeated calls run newest
    /// first. Like `listen`, a listener added during dispatch starts next event.
    pub fn listen_first(
        &self,
        node: NodeId,
        callback: impl FnMut(&mut EventContext) + 'static,
    ) -> InputBinding {
        self.listen_ordered(node, callback, true)
    }
    fn listen_ordered(
        &self,
        node: NodeId,
        callback: impl FnMut(&mut EventContext) + 'static,
        first: bool,
    ) -> InputBinding {
        let mut state = self.state.borrow_mut();
        state.next = state
            .next
            .checked_add(1)
            .expect("input binding IDs exhausted");
        let token = state.next;
        state.options.entry(node).or_default();
        let entry = Entry {
            token,
            callback: Rc::new(RefCell::new(callback)),
        };
        let entries = state.entries.entry(node).or_default();
        if first {
            entries.insert(0, entry);
        } else {
            entries.push(entry);
        }
        InputBinding {
            state: Rc::downgrade(&self.state),
            node,
            token,
        }
    }
    pub fn has_listeners(&self, node: NodeId) -> bool {
        self.state.borrow().entries.contains_key(&node)
    }
    /// `has_listeners`, or `None` while dispatch holds the input state (a
    /// native hit test can ask re-entrantly, from inside a callback).
    pub fn try_has_listeners(&self, node: NodeId) -> Option<bool> {
        let state = self.state.try_borrow().ok()?;
        Some(state.entries.contains_key(&node))
    }
    /// `options`, or `None` while dispatch holds the input state.
    pub fn try_options(&self, node: NodeId) -> Option<Option<NodeInput>> {
        let state = self.state.try_borrow().ok()?;
        Some(state.options.get(&node).copied())
    }
    /// `is_enabled`, or `None` while dispatch holds the input state.
    pub fn try_is_enabled(&self, scene: &Scene, node: NodeId) -> Option<bool> {
        drop(self.state.try_borrow().ok()?);
        Some(self.enabled(scene, node))
    }
    /// Metadata may be attached to containers without registering an event callback.
    pub fn set_options(&self, node: NodeId, options: NodeInput) {
        if options.disabled {
            self.actions.borrow_mut().cancel();
        }
        let observer = {
            let mut state = self.state.borrow_mut();
            state.options.insert(node, options);
            if options.disabled {
                state.clicks.0 = None;
            }
            state.disabled_observers.get(&node).cloned()
        };
        if let Some(observer) = observer {
            observer.set(options.disabled);
        }
    }
    /// Update disability and detach owned interactions before running cleanup
    /// callbacks. Disabled descendants still receive cancellation and blur.
    pub(crate) fn set_disabled(&self, scene: &Rc<RefCell<Scene>>, node: NodeId, disabled: bool) {
        let (captured, focused) = {
            let scene = scene.borrow();
            if !scene.contains(node) {
                return;
            }
            let inside = |id: NodeId| {
                scene.contains(id)
                    && (id == node || scene.ancestors(id).any(|ancestor| ancestor == node))
            };
            let mut state = self.state.borrow_mut();
            state.options.entry(node).or_default().disabled = disabled;
            if disabled {
                if state.clicks.0.is_some_and(|click| inside(click.target)) {
                    state.clicks.0 = None;
                }
                let captured = state.captured.filter(|id| inside(*id));
                let focused = state.focused.filter(|id| inside(*id));
                if captured.is_some() {
                    state.captured = None;
                    state.capture_button = None;
                }
                if state.pressed.is_some_and(inside) {
                    state.pressed = None;
                }
                if state
                    .key_pressed
                    .as_ref()
                    .is_some_and(|(id, _)| inside(*id))
                {
                    state.key_pressed = None;
                }
                if focused.is_some() {
                    state.focused = None;
                }
                (captured, focused)
            } else {
                (None, None)
            }
        };
        if let Some(target) = captured.filter(|id| scene.borrow().contains(*id)) {
            self.route(scene, target, InputEvent::PointerCancel);
        }
        if let Some(target) =
            focused.filter(|id| scene.borrow().contains(*id) && self.focused() != Some(*id))
        {
            self.route(scene, target, InputEvent::Blur);
        }
        // Cleanup handlers may remove or reenable the owner. Publish the
        // surviving metadata, never a stale value from before user callbacks.
        let notification = {
            let state = self.state.borrow();
            state.options.get(&node).and_then(|options| {
                state
                    .disabled_observers
                    .get(&node)
                    .map(|observer| (observer.clone(), options.disabled))
            })
        };
        if scene.borrow().contains(node)
            && let Some((observer, current)) = notification
        {
            observer.set(current);
        }
    }
    /// Lazily observe this node's local disabled flag. Ancestor inheritance is
    /// composed by callers; equal writes do not notify and unregister detaches it.
    pub(crate) fn observe_disabled(&self, node: NodeId, runtime: &Runtime) -> Signal<bool> {
        let mut state = self.state.borrow_mut();
        let disabled = state
            .options
            .get(&node)
            .is_some_and(|options| options.disabled);
        state
            .disabled_observers
            .entry(node)
            .or_insert_with(|| runtime.signal(disabled))
            .clone()
    }
    pub fn options(&self, node: NodeId) -> Option<NodeInput> {
        self.state.borrow().options.get(&node).copied()
    }
    /// Disconnect a removed node, including metadata-only container state.
    pub fn unregister(&self, node: NodeId) {
        let mut s = self.state.borrow_mut();
        let removed = s.entries.remove(&node);
        if let Some(entries) = &removed
            && s.drag.as_ref().map(|drag| drag.source).or(s.drag_ending) == Some(node)
        {
            let callbacks: Vec<Callback> =
                entries.iter().map(|entry| entry.callback.clone()).collect();
            s.drag_orphans.extend(callbacks);
        }
        let observer = s.disabled_observers.remove(&node);
        s.options.remove(&node);
        if removed.is_some() {
            crate::shrink_sparse(&mut s.entries);
            crate::shrink_sparse(&mut s.options);
        }
        s.layers.retain(|id| *id != node);
        if s.clicks.0.is_some_and(|click| click.target == node) {
            s.clicks.0 = None;
        }
        if s.focused == Some(node) {
            s.focused = None;
        }
        if s.captured == Some(node) {
            s.captured = None;
            s.capture_button = None;
        }
        if s.pressed == Some(node) {
            s.pressed = None;
        }
        if s.key_pressed.as_ref().is_some_and(|(id, _)| *id == node) {
            s.key_pressed = None;
        }
        drop(s);
        drop(observer);
        drop(removed);
    }
    pub fn focused(&self) -> Option<NodeId> {
        self.state.borrow().focused
    }
    /// The node under the pointer that takes input (its hit target), while
    /// it still listens.
    pub fn hovered(&self) -> Option<NodeId> {
        let state = self.state.borrow();
        state
            .hover
            .first()
            .copied()
            .filter(|id| state.entries.contains_key(id))
    }
    /// Hover what is under the resting pointer now that layout moved: a
    /// button that slid away from it stops being hovered (and what slid
    /// under it starts) without waiting for the pointer to move. Nothing
    /// changes while a press holds the pointer or a drag is under way.
    pub fn refresh_hover(&self, scene: &Rc<RefCell<Scene>>) {
        let pointer = {
            let state = self.state.borrow();
            if state.captured.is_some() || state.drag.is_some() {
                return;
            }
            state.pointer
        };
        let Some((x, y)) = pointer else {
            return;
        };
        let hit = self.hit_target(&scene.borrow(), x, y);
        self.set_hover(scene, hit);
    }
    /// Move hover to `hit` and its ancestors, as the DOM's `mouseenter` and
    /// `mouseleave` do: a node hears `PointerLeave` once the pointer is
    /// outside it and all its descendants, and `PointerEnter` once it comes
    /// inside; moving between descendants tells it nothing. Each node hears
    /// its own, at the target phase (they do not bubble): the nodes left,
    /// deepest first, then the nodes entered, outermost first. Costs a walk
    /// up from `hit` when the hit target changes, nothing when it does not.
    fn set_hover(&self, scene: &Rc<RefCell<Scene>>, hit: Option<NodeId>) {
        if self.state.borrow().hover.first().copied() == hit {
            return;
        }
        let mut next = Vec::new();
        {
            let scene = scene.borrow();
            let scope = self.focus_scope();
            let mut node = hit.filter(|id| scene.contains(*id));
            while let Some(id) = node {
                next.push(id);
                if Some(id) == scope {
                    break;
                }
                node = scene.parent(id);
            }
        }
        let entering = {
            let mut state = self.state.borrow_mut();
            let old = std::mem::take(&mut state.hover);
            let common = old
                .iter()
                .rev()
                .zip(next.iter().rev())
                .take_while(|(a, b)| a == b)
                .count();
            let entering: Vec<NodeId> = next[..next.len() - common].to_vec();
            state.hover = next;
            drop(state);
            for &id in &old[..old.len() - common] {
                self.hover_event(scene, id, InputEvent::PointerLeave);
            }
            entering
        };
        // A leave handler that moved hover again (it dispatched) owns it now.
        if self.state.borrow().hover.first().copied() != hit {
            return;
        }
        for &id in entering.iter().rev() {
            self.hover_event(scene, id, InputEvent::PointerEnter);
        }
    }
    /// Deliver an enter or leave to `node`'s own listeners only.
    fn hover_event(&self, scene: &Rc<RefCell<Scene>>, node: NodeId, event: InputEvent) {
        if !scene.borrow().contains(node) {
            return;
        }
        let callbacks: Vec<_> = {
            let state = self.state.borrow();
            let Some(entries) = state.entries.get(&node) else {
                return;
            };
            // A disabled node is entered no more; leaving still cleans up.
            if matches!(event, InputEvent::PointerEnter)
                && state.options.get(&node).is_some_and(|o| o.disabled)
            {
                return;
            }
            entries
                .iter()
                .map(|e| (e.token, e.callback.clone()))
                .collect()
        };
        let mut ctx = EventContext::new(event, node);
        for (token, callback) in callbacks {
            let attached = self
                .state
                .borrow()
                .entries
                .get(&node)
                .is_some_and(|entries| entries.iter().any(|entry| entry.token == token));
            if !attached || !scene.borrow().contains(node) {
                continue;
            }
            if let Ok(mut callback) = callback.try_borrow_mut() {
                callback(&mut ctx);
            }
            if ctx.immediate {
                break;
            }
        }
    }
    pub fn is_enabled(&self, scene: &Scene, node: NodeId) -> bool {
        self.enabled(scene, node)
    }
    pub fn captured(&self) -> Option<NodeId> {
        self.state.borrow().captured
    }
    fn set_capture(&self, node: Option<NodeId>, event: &InputEvent) {
        let mut state = self.state.borrow_mut();
        state.captured = node;
        state.capture_button = node.map(|_| match event {
            InputEvent::PointerDown { button, .. } => *button,
            _ => state.capture_button.unwrap_or(PointerButton::Primary),
        });
    }
    /// The active modal root, if any. Focus and pointer routing stay inside it.
    /// Make `root` a non-modal layer (toasts): what it holds takes the
    /// pointer and keys even while a modal holds focus elsewhere.
    pub fn set_layer(&self, root: NodeId, layer: bool) {
        let mut s = self.state.borrow_mut();
        s.layers.retain(|id| *id != root);
        if layer {
            s.layers.push(root);
        }
    }
    pub fn focus_scope(&self) -> Option<NodeId> {
        self.state.borrow().scopes.last().map(|s| s.root)
    }
    /// Enter a nested modal scope and save the previous focus for restoration.
    pub fn push_focus_scope(&self, scene: &Rc<RefCell<Scene>>, root: NodeId) -> bool {
        self.push_scope(scene, root, false)
    }
    /// As `push_focus_scope`, leaving the scope below live to the pointer
    /// and keys too: a submenu, whose parent menu's items stay hoverable.
    pub fn push_porous_focus_scope(&self, scene: &Rc<RefCell<Scene>>, root: NodeId) -> bool {
        self.push_scope(scene, root, true)
    }
    fn push_scope(&self, scene: &Rc<RefCell<Scene>>, root: NodeId, porous: bool) -> bool {
        // Opening a sibling modal may supersede the current scope. Check the
        // node's actual ancestry, without applying the active scope boundary.
        if !self.enabled_without_scope(&scene.borrow(), root)
            || self
                .state
                .borrow()
                .scopes
                .iter()
                .any(|scope| scope.root == root)
        {
            return false;
        }
        self.dispatch(scene, InputEvent::PointerCancel);
        if !self.enabled_without_scope(&scene.borrow(), root) {
            return false;
        }
        let previous = self.focused();
        self.state.borrow_mut().scopes.push(FocusScope {
            root,
            previous,
            porous,
        });
        let next = self.next_focus(&scene.borrow(), false);
        self.focus(scene, next);
        scene.borrow().contains(root)
            && self
                .state
                .borrow()
                .scopes
                .iter()
                .any(|scope| scope.root == root)
    }
    /// Close the innermost scope and restore the previous mounted, enabled focus.
    pub fn pop_focus_scope(&self, scene: &Rc<RefCell<Scene>>) -> bool {
        let scope = self.state.borrow_mut().scopes.pop();
        let Some(scope) = scope else {
            return false;
        };
        self.dispatch(scene, InputEvent::PointerCancel);
        if scene.borrow().contains(scope.root) {
            self.route(scene, scope.root, InputEvent::FocusScopeClosed);
        }
        let previous = scope
            .previous
            .filter(|id| self.enabled(&scene.borrow(), *id));
        let next = previous.or_else(|| self.next_focus(&scene.borrow(), false));
        self.focus(scene, next);
        true
    }
    /// Remove a modal scope and nested scopes, including one below an active
    /// sibling. Preserves the top sibling's focus and repairs restoration links.
    pub fn remove_focus_scope(&self, scene: &Rc<RefCell<Scene>>, root: NodeId) -> bool {
        let inside = |candidate: NodeId| {
            let scene = scene.borrow();
            candidate == root
                || (scene.contains(candidate)
                    && scene.ancestors(candidate).any(|ancestor| ancestor == root))
        };
        let mut removed = false;
        loop {
            let index = self
                .state
                .borrow()
                .scopes
                .iter()
                .rposition(|scope| inside(scope.root));
            let Some(index) = index else {
                break;
            };
            removed = true;
            if index + 1 == self.state.borrow().scopes.len() {
                self.pop_focus_scope(scene);
                continue;
            }
            let scope = self.state.borrow_mut().scopes.remove(index);
            {
                let scene = scene.borrow();
                let mut state = self.state.borrow_mut();
                for next in &mut state.scopes[index..] {
                    if next.previous.is_some_and(|previous| {
                        previous == scope.root
                            || (scene.contains(previous)
                                && scene
                                    .ancestors(previous)
                                    .any(|ancestor| ancestor == scope.root))
                    }) {
                        next.previous = scope.previous;
                    }
                }
            }
            if scene.borrow().contains(scope.root) {
                self.route(scene, scope.root, InputEvent::FocusScopeClosed);
            }
        }
        removed
    }
    fn enabled_without_scope(&self, scene: &Scene, mut node: NodeId) -> bool {
        if !scene.layout_visible(node) {
            return false;
        }
        let state = self.state.borrow();
        loop {
            if state
                .options
                .get(&node)
                .is_some_and(|options| options.disabled)
                || scene.effects(node).opacity <= 0.
            {
                return false;
            }
            match scene.parent(node) {
                Some(parent) => node = parent,
                None => return true,
            }
        }
    }
    fn enabled(&self, scene: &Scene, mut node: NodeId) -> bool {
        if !scene.layout_visible(node) {
            return false;
        }
        let s = self.state.borrow();
        // A non-modal layer's content is live whatever scope is active.
        let in_layer = !s.layers.is_empty() && {
            let mut ancestor = Some(node);
            let mut found = false;
            while let Some(id) = ancestor {
                if s.layers.contains(&id) {
                    found = true;
                    break;
                }
                ancestor = scene.parent(id);
            }
            found
        };
        // Inside the active scope, or, through porous scopes (submenus), any
        // scope below it.
        let mut live = s.scopes.iter().rev().peekable();
        if !in_layer && live.peek().is_some() {
            let mut inside = false;
            for scope in live {
                let mut ancestor = Some(node);
                while let Some(id) = ancestor {
                    if id == scope.root {
                        inside = true;
                        break;
                    }
                    ancestor = scene.parent(id);
                }
                if inside || !scope.porous {
                    break;
                }
            }
            if !inside {
                return false;
            }
        }
        loop {
            if s.options.get(&node).is_some_and(|o| o.disabled) {
                return false;
            }
            match scene.parent(node) {
                Some(p) => node = p,
                None => return true,
            }
        }
    }
    pub fn focus(&self, scene: &Rc<RefCell<Scene>>, target: Option<NodeId>) -> bool {
        if target.is_some_and(|id| {
            !self.enabled(&scene.borrow(), id) || !self.options(id).is_some_and(|o| o.focusable)
        }) {
            return false;
        }
        let old = self.focused();
        if old == target {
            return false;
        }
        self.actions.borrow_mut().cancel();
        {
            let mut state = self.state.borrow_mut();
            state.focused = target;
            state.key_pressed = None;
        }
        if let Some(id) = old.filter(|id| scene.borrow().contains(*id)) {
            self.route(scene, id, InputEvent::Blur);
        }
        // Blur handlers may remove the destination or redirect focus.
        if let Some(id) =
            target.filter(|id| self.focused() == Some(*id) && self.enabled(&scene.borrow(), *id))
        {
            self.route(scene, id, InputEvent::Focus);
        }
        true
    }
    fn route(&self, scene: &Rc<RefCell<Scene>>, target: NodeId, event: InputEvent) -> EventContext {
        let count = u8::from(matches!(event, InputEvent::PointerDown { .. }));
        if count != 0 {
            self.state.borrow_mut().clicks.0 = None;
        }
        // An activation carries the keys held as it was made (a ⌘-click).
        let held = if matches!(event, InputEvent::Activate) {
            self.state.borrow().modifiers
        } else {
            Modifiers::default()
        };
        self.route_pointer(scene, target, event, held, count)
    }
    fn route_pointer(
        &self,
        scene: &Rc<RefCell<Scene>>,
        target: NodeId,
        event: InputEvent,
        modifiers: Modifiers,
        click_count: u8,
    ) -> EventContext {
        if matches!(event, InputEvent::Blur) {
            let mut state = self.state.borrow_mut();
            if state.clicks.0.is_some_and(|click| click.target == target) {
                state.clicks.0 = None;
            }
        }
        let pointer_modifiers = if matches!(
            event,
            InputEvent::PointerDown { .. }
                | InputEvent::PointerUp { .. }
                | InputEvent::PointerMove { .. }
                | InputEvent::Scroll { .. }
                | InputEvent::PointerEnter
                | InputEvent::PointerLeave
                | InputEvent::PointerCancel
                | InputEvent::Activate
        ) {
            modifiers
        } else {
            Modifiers::default()
        };
        let mut path = vec![target];
        {
            let scene = scene.borrow();
            let mut id = target;
            while let Some(parent) = scene.parent(id) {
                if Some(id) == self.focus_scope() {
                    break;
                }
                path.push(parent);
                id = parent;
            }
        }
        let key_code = matches!(event, InputEvent::KeyDown { .. } | InputEvent::KeyUp { .. })
            .then(|| self.state.borrow().key_code.clone())
            .flatten();
        let mut ctx = EventContext {
            pointer_modifiers,
            click_count,
            key_code,
            ..EventContext::new(event, target)
        };
        let phases = path
            .iter()
            .skip(1)
            .rev()
            .map(|id| (*id, EventPhase::Capture))
            .chain(std::iter::once((target, EventPhase::Target)))
            .chain(path.iter().skip(1).map(|id| (*id, EventPhase::Bubble)));
        // Snapshot the entire route before invoking user code: newly attached listeners
        // participate only in later events; detached listeners are skipped immediately.
        let routes: Vec<_> = {
            let state = self.state.borrow();
            phases
                .map(|(id, phase)| {
                    let callbacks: Vec<_> = state
                        .entries
                        .get(&id)
                        .into_iter()
                        .flatten()
                        .map(|e| (e.token, e.callback.clone()))
                        .collect();
                    (id, phase, callbacks)
                })
                .collect()
        };
        for (id, phase, callbacks) in routes {
            if !scene.borrow().contains(id) {
                continue;
            }
            let disabled = self.options(id).is_some_and(|o| o.disabled);
            if disabled
                && !matches!(&ctx.event, InputEvent::Drag(event) if matches!(event.phase, DragPhase::End | DragPhase::Leave))
                && !matches!(
                    ctx.event,
                    InputEvent::Blur
                        | InputEvent::Focus
                        | InputEvent::PointerCancel
                        | InputEvent::FocusScopeClosed
                )
            {
                continue;
            }
            ctx.current_target = id;
            ctx.phase = phase;
            for (token, callback) in callbacks {
                let attached = self
                    .state
                    .borrow()
                    .entries
                    .get(&id)
                    .is_some_and(|entries| entries.iter().any(|entry| entry.token == token));
                if !attached || !scene.borrow().contains(id) {
                    continue;
                }
                // Reentrant delivery to the same callback is skipped, never panics.
                if let Ok(mut callback) = callback.try_borrow_mut() {
                    callback(&mut ctx);
                }
                if ctx.immediate {
                    break;
                }
            }
            if ctx.stopped {
                break;
            }
        }
        ctx
    }
    fn next_focus(&self, scene: &Scene, reverse: bool) -> Option<NodeId> {
        let mut stack = vec![scene.root()];
        let mut ordered = Vec::new();
        while let Some(id) = stack.pop() {
            let options = self.options(id);
            if let Some(o) = options.filter(|o| o.focusable && o.tab_index >= 0)
                && self.enabled(scene, id)
            {
                ordered.push((id, o.tab_index));
            }
            stack.extend(scene.children(id).iter().rev().copied());
        }
        ordered.sort_by_key(|(_, i)| if *i == 0 { i32::MAX } else { *i });
        if ordered.is_empty() {
            return None;
        }
        let index = ordered
            .iter()
            .position(|(id, _)| Some(*id) == self.focused());
        let next = match (index, reverse) {
            (Some(i), false) => (i + 1) % ordered.len(),
            (Some(i), true) => (i + ordered.len() - 1) % ordered.len(),
            (None, false) => 0,
            (None, true) => ordered.len() - 1,
        };
        Some(ordered[next].0)
    }
    pub fn dispatch_to(
        &self,
        scene: &Rc<RefCell<Scene>>,
        target: NodeId,
        event: InputEvent,
    ) -> DispatchResult {
        let old = self.focused();
        if !self.enabled(&scene.borrow(), target) {
            return DispatchResult::default();
        }
        let ctx = self.route(scene, target, event);
        if let Some(focus) = ctx.focus {
            self.focus(scene, Some(focus));
        }
        if let Some(capture) = ctx.capture {
            let capture = capture.filter(|id| self.enabled(&scene.borrow(), *id));
            self.set_capture(capture, &ctx.event);
        }
        DispatchResult {
            target: Some(target),
            default_prevented: ctx.prevented,
            focus_changed: old != self.focused(),
        }
    }
    fn hit_target(&self, scene: &Scene, x: f32, y: f32) -> Option<NodeId> {
        let mut fallback = None;
        'hits: for hit in scene.hit_test_all(x, y) {
            let mut node = Some(hit);
            while let Some(id) = node {
                let options = self.options(id);
                if options.is_some_and(|options| options.pass_through) {
                    continue 'hits;
                }
                if options.is_some_and(|options| options.disabled) {
                    return None;
                }
                if self.has_listeners(id) {
                    // A document-level capture listener must not make an empty overlay
                    // obscure interactive siblings below it.
                    if id == scene.root() {
                        fallback = Some(id);
                        break;
                    }
                    return self.enabled(scene, id).then_some(id);
                }
                node = scene.parent(id);
            }
        }
        fallback.filter(|id| self.enabled(scene, *id))
    }
    pub fn dispatch(&self, scene: &Rc<RefCell<Scene>>, event: InputEvent) -> DispatchResult {
        self.dispatch_impl(scene, event, None, true)
    }
    /// Dispatch native-style pointer modifiers and multi-click recognition.
    /// Clicks cycle 1, 2, 3 within 500 ms and four logical pixels on one target/button.
    pub fn dispatch_with_modifiers(
        &self,
        scene: &Rc<RefCell<Scene>>,
        event: InputEvent,
        modifiers: Modifiers,
    ) -> DispatchResult {
        self.dispatch_impl(scene, event, Some(modifiers), true)
    }
    fn dispatch_impl(
        &self,
        scene: &Rc<RefCell<Scene>>,
        event: InputEvent,
        modifiers: Option<Modifiers>,
        bindings: bool,
    ) -> DispatchResult {
        if let Some(held) = modifiers {
            self.state.borrow_mut().modifiers = held;
        }
        if let InputEvent::KeyDown { modifiers, .. } | InputEvent::KeyUp { modifiers, .. } = &event
        {
            self.state.borrow_mut().modifiers = *modifiers;
        }
        match &event {
            InputEvent::PointerDown { .. } => self.state.borrow_mut().pointer_modality = true,
            InputEvent::KeyDown { modifiers, .. }
                if !modifiers.meta && !modifiers.control && !modifiers.alt =>
            {
                self.state.borrow_mut().pointer_modality = false
            }
            _ => {}
        }
        if self.dispatch_drag(scene, &event) {
            return DispatchResult {
                default_prevented: true,
                ..Default::default()
            };
        }
        {
            let mut state = self.state.borrow_mut();
            match &event {
                InputEvent::PointerCancel | InputEvent::PointerLeave | InputEvent::Blur => {
                    state.clicks.0 = None
                }
                InputEvent::PointerDown { .. } if modifiers.is_none() => state.clicks.0 = None,
                InputEvent::PointerMove { x, y } | InputEvent::PointerUp { x, y, .. } => {
                    state.clicks.moved(*x, *y)
                }
                _ => {}
            }
            match &event {
                InputEvent::PointerMove { x, y }
                | InputEvent::PointerDown { x, y, .. }
                | InputEvent::PointerUp { x, y, .. } => state.pointer = Some((*x, *y)),
                InputEvent::PointerLeave => state.pointer = None,
                _ => {}
            }
        }
        let previous = self.state.borrow().clicks.0.map(|click| click.target);
        if previous.is_some_and(|target| !self.enabled(&scene.borrow(), target)) {
            self.state.borrow_mut().clicks.0 = None;
        }
        loop {
            let stale = self
                .focus_scope()
                .is_some_and(|id| !scene.borrow().contains(id));
            if !stale {
                break;
            }
            self.pop_focus_scope(scene);
        }
        if matches!(event, InputEvent::Blur) {
            let mut s = self.state.borrow_mut();
            s.captured = None;
            s.capture_button = None;
            s.pressed = None;
            s.key_pressed = None;
        }
        let old_focus = self.focused();
        if old_focus.is_some_and(|id| !self.enabled(&scene.borrow(), id)) {
            self.focus(scene, None);
        }
        if matches!(
            event,
            InputEvent::Blur
                | InputEvent::PointerDown { .. }
                | InputEvent::ImePreedit { .. }
                | InputEvent::ImeCommit(_)
        ) {
            self.actions.borrow_mut().cancel();
        }
        if bindings {
            self.advance_key_sequence(scene, Instant::now());
            if self.actions.borrow_mut().retain_event(&event) {
                return DispatchResult {
                    target: self.focused(),
                    default_prevented: true,
                    focus_changed: false,
                };
            }
            if let InputEvent::KeyDown {
                key,
                modifiers,
                repeat,
            } = &event
            {
                loop {
                    let path = self.action_path(&scene.borrow());
                    let resolution = self.actions.borrow_mut().resolve(
                        path,
                        crate::actions::Keystroke::new(key.clone(), *modifiers),
                        *repeat,
                        Instant::now(),
                    );
                    match resolution {
                        crate::actions::Resolution::Matched(replay) => {
                            return self.replay_keys(scene, replay);
                        }
                        crate::actions::Resolution::Retry(replay) => {
                            self.replay_keys(scene, replay);
                        }
                        crate::actions::Resolution::Prefix => {
                            return DispatchResult {
                                target: self.focused(),
                                default_prevented: true,
                                focus_changed: false,
                            };
                        }
                        crate::actions::Resolution::None => break,
                    }
                }
            }
        }
        let capture = self
            .captured()
            .filter(|id| self.enabled(&scene.borrow(), *id));
        if capture.is_none() {
            self.set_capture(None, &event);
        }
        let hit = match &event {
            InputEvent::PointerDown { x, y, .. }
            | InputEvent::PointerUp { x, y, .. }
            | InputEvent::PointerMove { x, y }
            | InputEvent::Scroll { x, y, .. }
            | InputEvent::FileHover { x, y, .. }
            | InputEvent::FileDrop { x, y, .. }
            | InputEvent::FilesDrop { x, y, .. }
            | InputEvent::FilesDropRejected { x, y, .. } => {
                self.hit_target(&scene.borrow(), *x, *y)
            }
            _ => None,
        };
        if matches!(event, InputEvent::PointerMove { .. }) {
            let mut state = self.state.borrow_mut();
            if state
                .clicks
                .0
                .is_some_and(|click| Some(click.target) != hit)
            {
                state.clicks.0 = None;
            }
        }
        if matches!(
            event,
            InputEvent::PointerMove { .. } | InputEvent::PointerLeave
        ) {
            self.set_hover(scene, hit);
        }
        let target = match &event {
            InputEvent::PointerDown { .. }
            | InputEvent::PointerUp { .. }
            | InputEvent::PointerMove { .. } => capture.or(hit),
            InputEvent::Scroll { .. }
            | InputEvent::FileHover { .. }
            | InputEvent::FileDrop { .. }
            | InputEvent::FilesDrop { .. }
            | InputEvent::FilesDropRejected { .. } => hit,
            InputEvent::FileHoverCancelled => self.state.borrow_mut().file_hovered.take(),
            InputEvent::PointerCancel => capture,
            InputEvent::PointerLeave => None,
            // Empty or disabled modal contents still need dismissal keys.
            InputEvent::KeyDown { .. } | InputEvent::KeyUp { .. } => {
                self.focused().or_else(|| self.focus_scope())
            }
            _ => self.focused(),
        };
        if matches!(event, InputEvent::FileHover { .. }) {
            let old = self.state.borrow_mut().file_hovered.take();
            if old != target
                && let Some(old) = old.filter(|id| scene.borrow().contains(*id))
            {
                self.dispatch_to(scene, old, InputEvent::FileHoverCancelled);
            }
            self.state.borrow_mut().file_hovered = target;
        } else if matches!(
            event,
            InputEvent::FileDrop { .. }
                | InputEvent::FilesDrop { .. }
                | InputEvent::FilesDropRejected { .. }
        ) {
            self.state.borrow_mut().file_hovered = None;
        }
        let click_count = if let InputEvent::PointerDown { x, y, button } = &event {
            if modifiers.is_some() {
                if let Some(target) = target {
                    self.state
                        .borrow_mut()
                        .clicks
                        .press(target, *button, (*x, *y), Instant::now())
                } else {
                    self.state.borrow_mut().clicks.0 = None;
                    1
                }
            } else {
                1
            }
        } else {
            0
        };
        let mut prevented = false;
        if let Some(id) = target {
            let ctx = self.route_pointer(
                scene,
                id,
                event.clone(),
                modifiers.unwrap_or_default(),
                click_count,
            );
            prevented = ctx.prevented;
            if let Some((source, payload, pressed_at)) = ctx.drag_request.clone()
                && let InputEvent::PointerMove { x, y } = event
            {
                self.begin_drag(scene, source, payload, x, y, pressed_at.unwrap_or((x, y)));
            }
            if let Some(taker) = ctx.take_pointer {
                let (pressed, captured) = {
                    let mut state = self.state.borrow_mut();
                    (state.pressed.take(), state.captured)
                };
                let pressed = pressed.filter(|id| Some(*id) != captured);
                for id in [captured, pressed].into_iter().flatten() {
                    if id != taker && scene.borrow().contains(id) {
                        self.route(scene, id, InputEvent::PointerCancel);
                    }
                }
            }
            if let Some(capture) = ctx.capture {
                let capture = capture.filter(|id| self.enabled(&scene.borrow(), *id));
                self.set_capture(capture, &ctx.event);
            }
            if let Some(focus) = ctx.focus {
                self.focus(scene, Some(focus));
            }
        }
        if !prevented {
            match &event {
                InputEvent::PointerDown {
                    button: PointerButton::Primary,
                    ..
                } => {
                    self.state.borrow_mut().pressed = target;
                    let mut node = target.filter(|id| self.enabled(&scene.borrow(), *id));
                    let mut focus = None;
                    while let Some(id) = node {
                        if self.options(id).is_some_and(|o| o.focusable) {
                            focus = Some(id);
                            break;
                        }
                        node = scene.borrow().parent(id);
                    }
                    if target.is_some() || self.focus_scope().is_none() {
                        self.focus(scene, focus);
                    }
                }
                InputEvent::KeyDown {
                    key: Key::Tab,
                    modifiers,
                    ..
                } => {
                    let next = self.next_focus(&scene.borrow(), modifiers.shift);
                    self.focus(scene, next);
                    prevented = true;
                }
                InputEvent::PointerUp {
                    button: PointerButton::Primary,
                    ..
                } => {
                    let pressed = self.state.borrow_mut().pressed.take();
                    if let Some(id) =
                        pressed.filter(|id| Some(*id) == hit && self.enabled(&scene.borrow(), *id))
                    {
                        self.dispatch_to(scene, id, InputEvent::Activate);
                    }
                }
                InputEvent::KeyDown {
                    key: key @ (Key::Enter | Key::Space),
                    repeat: false,
                    ..
                } => {
                    let pressed = self.focused().map(|id| (id, key.clone()));
                    self.state.borrow_mut().key_pressed = pressed;
                }
                InputEvent::KeyUp { key, .. } => {
                    let pressed = {
                        let mut state = self.state.borrow_mut();
                        if state
                            .key_pressed
                            .as_ref()
                            .is_some_and(|(_, pressed)| key == pressed)
                        {
                            state.key_pressed.take()
                        } else {
                            None
                        }
                    };
                    if let Some((id, _)) = pressed
                        && Some(id) == self.focused()
                    {
                        self.dispatch_to(scene, id, InputEvent::Activate);
                    }
                }
                _ => {}
            }
        }
        {
            let mut state = self.state.borrow_mut();
            if matches!(event, InputEvent::PointerCancel)
                || matches!(event, InputEvent::PointerUp { button, .. } if Some(button) == state.capture_button)
            {
                state.captured = None;
                state.capture_button = None;
            }
            if matches!(
                event,
                InputEvent::PointerCancel
                    | InputEvent::PointerUp {
                        button: PointerButton::Primary,
                        ..
                    }
            ) {
                state.pressed = None;
            }
        }
        if prevented && let InputEvent::KeyUp { key, .. } = &event {
            let mut state = self.state.borrow_mut();
            if state
                .key_pressed
                .as_ref()
                .is_some_and(|(_, pressed)| key == pressed)
            {
                state.key_pressed = None;
            }
        }
        DispatchResult {
            target,
            default_prevented: prevented,
            focus_changed: old_focus != self.focused(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{Color, NodeKind, Style};
    fn setup() -> (Rc<RefCell<Scene>>, NodeId, NodeId) {
        let mut s = Scene::new(200.0, 200.0);
        let root = s.root();
        let a = s.append(
            root,
            NodeKind::Rect(Color(0, 0, 0, 255)),
            Style {
                width: Some(100.0),
                height: Some(40.0),
                ..Default::default()
            },
        );
        let b = s.append(
            root,
            NodeKind::Rect(Color(0, 0, 0, 255)),
            Style {
                width: Some(100.0),
                height: Some(40.0),
                ..Default::default()
            },
        );
        s.flush();
        (Rc::new(RefCell::new(s)), a, b)
    }
    #[test]
    fn click_tracker_time_distance_button_target_and_cycle_are_deterministic() {
        let (_, a, b) = setup();
        let mut tracker = ClickTracker::default();
        let now = Instant::now();
        for (index, expected) in [1, 2, 3, 1].into_iter().enumerate() {
            assert_eq!(
                tracker.press(
                    a,
                    PointerButton::Primary,
                    (0., 0.),
                    now + Duration::from_millis(index as u64 * 500)
                ),
                expected
            );
        }
        assert_eq!(
            tracker.press(
                a,
                PointerButton::Primary,
                (4., 0.),
                now + Duration::from_millis(2000)
            ),
            2
        );
        assert_eq!(
            tracker.press(
                a,
                PointerButton::Primary,
                (4., 0.),
                now + Duration::from_millis(2501)
            ),
            1
        );
        assert_eq!(
            tracker.press(
                b,
                PointerButton::Primary,
                (4., 0.),
                now + Duration::from_millis(2502)
            ),
            1
        );
        assert_eq!(
            tracker.press(
                b,
                PointerButton::Secondary,
                (4., 0.),
                now + Duration::from_millis(2503)
            ),
            1
        );
        tracker.moved(8.01, 0.);
        assert_eq!(
            tracker.press(
                b,
                PointerButton::Secondary,
                (4., 0.),
                now + Duration::from_millis(2504)
            ),
            1
        );
        tracker.moved(f32::NAN, 0.);
        assert!(tracker.0.is_none());
    }

    #[test]
    fn pointer_metadata_routes_explicitly_and_legacy_dispatch_resets_chain() {
        let (scene, a, b) = setup();
        let input = InputDispatcher::new();
        let log = Rc::new(RefCell::new(Vec::new()));
        let _b = input.register(
            b,
            NodeInput {
                focusable: true,
                ..Default::default()
            },
            move |cx| {
                assert_eq!(cx.pointer_modifiers(), Modifiers::default());
                assert_eq!(cx.click_count(), 0);
            },
        );
        input.focus(&scene, Some(b));
        let nested_input = input.clone();
        let nested_scene = scene.clone();
        let outer_log = log.clone();
        let _a = input.register(
            a,
            NodeInput {
                focusable: true,
                ..Default::default()
            },
            move |cx| {
                if matches!(cx.event, InputEvent::PointerDown { .. }) {
                    let before = (cx.pointer_modifiers(), cx.click_count());
                    // An explicitly routed nested event has independent legacy metadata.
                    nested_input.dispatch_to(
                        &nested_scene,
                        b,
                        InputEvent::PointerMove { x: 1., y: 45. },
                    );
                    assert_eq!((cx.pointer_modifiers(), cx.click_count()), before);
                    outer_log.borrow_mut().push(("outer", before.0, before.1));
                }
            },
        );
        let shift = Modifiers {
            shift: true,
            ..Default::default()
        };
        input.dispatch_with_modifiers(&scene, down(2., 2.), shift);
        input.dispatch_with_modifiers(&scene, up(2., 2.), shift);
        input.dispatch_with_modifiers(&scene, down(2., 2.), shift);
        input.dispatch(&scene, down(2., 2.));
        input.dispatch_with_modifiers(&scene, down(2., 2.), shift);
        assert_eq!(
            &*log.borrow(),
            &[
                ("outer", shift, 1),
                ("outer", shift, 2),
                ("outer", Modifiers::default(), 1),
                ("outer", shift, 1)
            ]
        );
        for reset in [
            InputEvent::PointerCancel,
            InputEvent::PointerLeave,
            InputEvent::Blur,
            InputEvent::PointerMove { x: 50., y: 2. },
        ] {
            input.dispatch_with_modifiers(&scene, reset, shift);
            input.dispatch_with_modifiers(&scene, down(2., 2.), shift);
            assert_eq!(log.borrow().last().unwrap().2, 1);
        }
        input.set_disabled(&scene, a, true);
        input.set_disabled(&scene, a, false);
        input.dispatch_with_modifiers(&scene, down(2., 2.), shift);
        assert_eq!(log.borrow().last().unwrap().2, 1);
        input.unregister(a);
        assert!(input.state.borrow().clicks.0.is_none());
    }
    #[test]
    fn prepended_listener_preserves_options_and_restores_default_on_drop() {
        let (scene, node, _) = setup();
        let input = InputDispatcher::new();
        let log = Rc::new(RefCell::new(Vec::new()));
        let default_log = log.clone();
        let _default = input.register(
            node,
            NodeInput {
                focusable: true,
                tab_index: 7,
                ..Default::default()
            },
            move |cx| {
                if matches!(cx.event, InputEvent::Activate) {
                    default_log
                        .borrow_mut()
                        .push(("default", cx.default_prevented()));
                }
            },
        );
        let user_log = log.clone();
        let first = input.listen_first(node, move |cx| {
            if matches!(cx.event, InputEvent::Activate) {
                user_log.borrow_mut().push(("user", cx.default_prevented()));
                cx.prevent_default();
            }
        });
        assert!(input.options(node).unwrap().focusable);
        assert_eq!(input.options(node).unwrap().tab_index, 7);
        input.dispatch_to(&scene, node, InputEvent::Activate);
        assert_eq!(&*log.borrow(), &[("user", false), ("default", true)]);
        drop(first);
        log.borrow_mut().clear();
        input.dispatch_to(&scene, node, InputEvent::Activate);
        assert_eq!(&*log.borrow(), &[("default", false)]);
    }
    fn down(x: f32, y: f32) -> InputEvent {
        InputEvent::PointerDown {
            x,
            y,
            button: PointerButton::Primary,
        }
    }
    fn up(x: f32, y: f32) -> InputEvent {
        InputEvent::PointerUp {
            x,
            y,
            button: PointerButton::Primary,
        }
    }
    #[test]
    fn removal_during_dispatch_does_not_leave_stale_focus() {
        let (scene, a, _) = setup();
        let d = InputDispatcher::new();
        let s = scene.clone();
        let _binding = d.register(
            a,
            NodeInput {
                focusable: true,
                ..Default::default()
            },
            move |event| {
                if matches!(event.event, InputEvent::PointerDown { .. }) {
                    s.borrow_mut().remove(a);
                }
            },
        );
        d.dispatch(&scene, down(10.0, 10.0));
        assert_eq!(d.focused(), None);
        d.dispatch(&scene, up(10.0, 10.0));
    }
    #[test]
    fn capture_target_bubble_and_raii() {
        let (scene, a, _) = setup();
        let d = InputDispatcher::new();
        let log = Rc::new(RefCell::new(Vec::new()));
        let l = log.clone();
        let root = scene.borrow().root();
        let _r = d.register(root, NodeInput::default(), move |c| {
            l.borrow_mut().push(c.phase)
        });
        let l = log.clone();
        let binding = d.register(a, NodeInput::default(), move |c| {
            l.borrow_mut().push(c.phase)
        });
        d.dispatch(&scene, down(10.0, 10.0));
        assert_eq!(
            *log.borrow(),
            vec![EventPhase::Capture, EventPhase::Target, EventPhase::Bubble]
        );
        drop(binding);
        log.borrow_mut().clear();
        d.dispatch(&scene, down(10.0, 10.0));
        assert_eq!(*log.borrow(), vec![EventPhase::Target]);
    }
    #[test]
    fn tab_focus_disabled_and_stale_nodes() {
        let (scene, a, b) = setup();
        let d = InputDispatcher::new();
        let opts = NodeInput {
            focusable: true,
            ..Default::default()
        };
        let _a = d.register(a, opts, |_| {});
        let _b = d.register(b, opts, |_| {});
        let tab = InputEvent::KeyDown {
            key: Key::Tab,
            modifiers: Modifiers::default(),
            repeat: false,
        };
        d.dispatch(&scene, tab.clone());
        assert_eq!(d.focused(), Some(a));
        d.dispatch(&scene, tab.clone());
        assert_eq!(d.focused(), Some(b));
        d.set_options(
            a,
            NodeInput {
                disabled: true,
                ..opts
            },
        );
        d.dispatch(&scene, tab.clone());
        assert_eq!(d.focused(), Some(b));
        scene.borrow_mut().remove(b);
        d.dispatch(&scene, tab);
        assert_eq!(d.focused(), None);
    }
    #[test]
    fn disabling_ancestor_cancels_unfocused_press_before_reenable() {
        let (scene, a, _) = setup();
        let input = InputDispatcher::new();
        let count = Rc::new(std::cell::Cell::new(0));
        let clicks = count.clone();
        let _binding = input.register(a, NodeInput::default(), move |cx| {
            if cx.event == InputEvent::Activate {
                clicks.set(clicks.get() + 1);
            }
        });
        input.dispatch(&scene, down(10., 10.));
        assert_eq!(input.focused(), None);
        assert_eq!(input.captured(), None);
        let root = scene.borrow().root();
        input.set_disabled(&scene, root, true);
        input.set_disabled(&scene, root, false);
        input.dispatch(&scene, up(10., 10.));
        assert_eq!(count.get(), 0);
    }

    #[test]
    fn disabling_detaches_interactions_before_callbacks_and_observes_reentrant_state() {
        let (scene, a, _) = setup();
        let input = InputDispatcher::new();
        let root = scene.borrow().root();
        let runtime = Runtime::new();
        let disabled = input.observe_disabled(root, &runtime);
        let dispatcher = input.clone();
        let callback_scene = scene.clone();
        let log = Rc::new(RefCell::new(Vec::new()));
        let events = log.clone();
        let _binding = input.register(
            a,
            NodeInput {
                focusable: true,
                ..Default::default()
            },
            move |cx| match cx.event {
                InputEvent::PointerDown { .. } => cx.capture_pointer(),
                InputEvent::PointerCancel | InputEvent::Blur => {
                    assert_eq!(dispatcher.captured(), None);
                    assert_eq!(dispatcher.focused(), None);
                    assert!(
                        !dispatcher.focus(&callback_scene, Some(a)),
                        "disabled target must not refocus"
                    );
                    events.borrow_mut().push(cx.event.clone());
                    if cx.event == InputEvent::Blur {
                        dispatcher.set_disabled(&callback_scene, root, false);
                    }
                }
                _ => {}
            },
        );
        input.dispatch(&scene, down(10., 10.));
        input.dispatch(
            &scene,
            InputEvent::KeyDown {
                key: Key::Space,
                repeat: false,
                modifiers: Modifiers::default(),
            },
        );
        input.set_disabled(&scene, root, true);
        assert_eq!(
            &*log.borrow(),
            &[InputEvent::PointerCancel, InputEvent::Blur]
        );
        assert!(!disabled.get());
        assert!(!input.options(root).unwrap().disabled);
        assert!(input.state.borrow().pressed.is_none());
        assert!(input.state.borrow().key_pressed.is_none());
    }

    #[test]
    fn cancellation_reenable_and_refocus_does_not_blur_new_focus() {
        let (scene, a, _) = setup();
        let input = InputDispatcher::new();
        let dispatcher = input.clone();
        let callback_scene = scene.clone();
        let focused_style = Rc::new(std::cell::Cell::new(false));
        let visual = focused_style.clone();
        let _binding = input.register(
            a,
            NodeInput {
                focusable: true,
                ..Default::default()
            },
            move |cx| match cx.event {
                InputEvent::PointerDown { .. } => cx.capture_pointer(),
                InputEvent::PointerCancel => {
                    dispatcher.set_disabled(&callback_scene, a, false);
                    dispatcher.focus(&callback_scene, Some(a));
                }
                InputEvent::Focus => visual.set(true),
                InputEvent::Blur => visual.set(false),
                _ => {}
            },
        );
        input.dispatch(&scene, down(10., 10.));
        assert!(focused_style.get());
        input.set_disabled(&scene, a, true);
        assert_eq!(input.focused(), Some(a));
        assert!(
            focused_style.get(),
            "old deferred blur must not clear regained focus"
        );
    }

    #[test]
    fn disable_cleanup_may_remove_target_without_stale_notification_or_blur() {
        let (scene, a, _) = setup();
        let input = InputDispatcher::new();
        let disabled = input.observe_disabled(a, &Runtime::new());
        let dispatcher = input.clone();
        let callback_scene = scene.clone();
        let _binding = input.register(
            a,
            NodeInput {
                focusable: true,
                ..Default::default()
            },
            move |cx| match cx.event {
                InputEvent::PointerDown { .. } => cx.capture_pointer(),
                InputEvent::PointerCancel => {
                    dispatcher.unregister(a);
                    callback_scene.borrow_mut().remove(a);
                }
                InputEvent::Blur => panic!("removed target received blur"),
                _ => {}
            },
        );
        input.dispatch(&scene, down(10., 10.));
        input.set_disabled(&scene, a, true);
        assert!(!scene.borrow().contains(a));
        assert_eq!(input.focused(), None);
        assert_eq!(input.captured(), None);
        assert!(input.options(a).is_none());
        assert!(
            !disabled.get(),
            "removed observer must not receive stale disabled notification"
        );
    }

    #[test]
    fn activation_key_release_ignores_unrelated_keys_and_repeat_but_honors_owner() {
        let (scene, a, _) = setup();
        let input = InputDispatcher::new();
        let clicks = Rc::new(std::cell::Cell::new(0));
        let count = clicks.clone();
        let _binding = input.register(
            a,
            NodeInput {
                focusable: true,
                ..Default::default()
            },
            move |cx| {
                if cx.event == InputEvent::Activate {
                    count.set(count.get() + 1);
                }
            },
        );
        input.focus(&scene, Some(a));
        let down = |key, repeat| InputEvent::KeyDown {
            key,
            repeat,
            modifiers: Modifiers::default(),
        };
        let up = |key| InputEvent::KeyUp {
            key,
            modifiers: Modifiers::default(),
        };
        input.dispatch(&scene, down(Key::Space, false));
        input.dispatch(&scene, up(Key::Character("x".into())));
        input.dispatch(&scene, down(Key::Space, true));
        assert_eq!(clicks.get(), 0);
        input.dispatch(&scene, up(Key::Space));
        input.dispatch(&scene, up(Key::Space));
        assert_eq!(clicks.get(), 1);
        // The latest non-repeat activation key owns the one pending activation.
        input.dispatch(&scene, down(Key::Space, false));
        input.dispatch(&scene, down(Key::Enter, false));
        input.dispatch(&scene, down(Key::Space, true));
        input.dispatch(&scene, up(Key::Space));
        assert_eq!(clicks.get(), 1);
        input.dispatch(&scene, up(Key::Enter));
        assert_eq!(clicks.get(), 2);
    }

    #[test]
    fn activation_callback_can_start_next_same_key_press() {
        let (scene, a, _) = setup();
        let input = InputDispatcher::new();
        let clicks = Rc::new(std::cell::Cell::new(0));
        let count = clicks.clone();
        let dispatcher = input.clone();
        let callback_scene = scene.clone();
        let _binding = input.register(
            a,
            NodeInput {
                focusable: true,
                ..Default::default()
            },
            move |cx| {
                if cx.event == InputEvent::Activate {
                    count.set(count.get() + 1);
                    if count.get() == 1 {
                        dispatcher.dispatch(
                            &callback_scene,
                            InputEvent::KeyDown {
                                key: Key::Space,
                                repeat: false,
                                modifiers: Modifiers::default(),
                            },
                        );
                    }
                }
            },
        );
        input.focus(&scene, Some(a));
        input.dispatch(
            &scene,
            InputEvent::KeyDown {
                key: Key::Space,
                repeat: false,
                modifiers: Modifiers::default(),
            },
        );
        let up = InputEvent::KeyUp {
            key: Key::Space,
            modifiers: Modifiers::default(),
        };
        input.dispatch(&scene, up.clone());
        input.dispatch(&scene, up);
        assert_eq!(clicks.get(), 2);
    }

    #[test]
    fn prevented_matching_key_release_and_blur_discard_pending_activation() {
        let (scene, a, _) = setup();
        let input = InputDispatcher::new();
        let clicks = Rc::new(std::cell::Cell::new(0));
        let prevent = Rc::new(std::cell::Cell::new(true));
        let count = clicks.clone();
        let cancel = prevent.clone();
        let _binding = input.register(
            a,
            NodeInput {
                focusable: true,
                ..Default::default()
            },
            move |cx| {
                if matches!(
                    cx.event,
                    InputEvent::KeyUp {
                        key: Key::Space,
                        ..
                    }
                ) && cancel.get()
                {
                    cx.prevent_default();
                }
                if cx.event == InputEvent::Activate {
                    count.set(count.get() + 1);
                }
            },
        );
        input.focus(&scene, Some(a));
        let down = InputEvent::KeyDown {
            key: Key::Space,
            repeat: false,
            modifiers: Modifiers::default(),
        };
        let up = InputEvent::KeyUp {
            key: Key::Space,
            modifiers: Modifiers::default(),
        };
        input.dispatch(&scene, down.clone());
        input.dispatch(&scene, up.clone());
        assert!(input.state.borrow().key_pressed.is_none());
        prevent.set(false);
        input.dispatch(&scene, up.clone());
        input.dispatch(&scene, down);
        input.dispatch(&scene, InputEvent::Blur);
        input.dispatch(&scene, up);
        assert_eq!(clicks.get(), 0);
    }

    #[test]
    fn unrelated_button_release_preserves_capture_and_primary_activation() {
        for initiating in [PointerButton::Primary, PointerButton::Secondary] {
            let (scene, a, _) = setup();
            let input = InputDispatcher::new();
            let clicks = Rc::new(std::cell::Cell::new(0));
            let count = clicks.clone();
            let _binding = input.register(a, NodeInput::default(), move |cx| {
                if matches!(cx.event, InputEvent::PointerDown { button, .. } if button == initiating) {
                    cx.capture_pointer();
                }
                if cx.event == InputEvent::Activate { count.set(count.get() + 1); }
            });
            input.dispatch(
                &scene,
                InputEvent::PointerDown {
                    x: 10.,
                    y: 10.,
                    button: initiating,
                },
            );
            let unrelated = if initiating == PointerButton::Primary {
                PointerButton::Secondary
            } else {
                PointerButton::Primary
            };
            input.dispatch(
                &scene,
                InputEvent::PointerUp {
                    x: 10.,
                    y: 10.,
                    button: unrelated,
                },
            );
            assert_eq!(input.captured(), Some(a));
            assert_eq!(
                input
                    .dispatch(&scene, InputEvent::PointerMove { x: 190., y: 190. })
                    .target,
                Some(a)
            );
            input.dispatch(
                &scene,
                InputEvent::PointerUp {
                    x: 10.,
                    y: 10.,
                    button: initiating,
                },
            );
            assert_eq!(input.captured(), None);
            assert_eq!(
                clicks.get(),
                usize::from(initiating == PointerButton::Primary)
            );
        }
    }

    #[test]
    fn prevented_release_and_cancel_clear_capture_without_delayed_activation() {
        let (scene, a, _) = setup();
        let input = InputDispatcher::new();
        let clicks = Rc::new(std::cell::Cell::new(0));
        let count = clicks.clone();
        let _binding = input.register(a, NodeInput::default(), move |cx| match cx.event {
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                ..
            } => cx.capture_pointer(),
            InputEvent::PointerUp {
                button: PointerButton::Primary,
                ..
            } => cx.prevent_default(),
            InputEvent::Activate => count.set(count.get() + 1),
            _ => {}
        });
        input.dispatch(&scene, down(10., 10.));
        input.dispatch(&scene, up(10., 10.));
        assert_eq!(input.captured(), None);
        assert_eq!(input.state.borrow().pressed, None);
        input.dispatch(&scene, down(10., 10.));
        input.dispatch(&scene, InputEvent::PointerCancel);
        assert_eq!(input.captured(), None);
        assert_eq!(input.state.borrow().capture_button, None);
        assert_eq!(input.state.borrow().pressed, None);
        input.dispatch(&scene, up(10., 10.));
        assert_eq!(clicks.get(), 0);
    }

    #[test]
    fn capture_tracks_drag_but_does_not_activate_outside() {
        let (scene, a, _) = setup();
        let d = InputDispatcher::new();
        let clicks = Rc::new(RefCell::new(0));
        let c = clicks.clone();
        let _a = d.register(a, NodeInput::default(), move |e| {
            if matches!(e.event, InputEvent::PointerDown { .. }) {
                e.capture_pointer();
            }
            if e.event == InputEvent::Activate {
                *c.borrow_mut() += 1;
            }
        });
        d.dispatch(&scene, up(10.0, 10.0));
        assert_eq!(*clicks.borrow(), 0);
        d.dispatch(&scene, down(10.0, 10.0));
        assert_eq!(d.captured(), Some(a));
        let result = d.dispatch(&scene, InputEvent::PointerMove { x: 190.0, y: 190.0 });
        assert_eq!(result.target, Some(a));
        d.dispatch(&scene, up(190.0, 190.0));
        assert_eq!(*clicks.borrow(), 0);
        assert_eq!(d.captured(), None);
        d.dispatch(&scene, down(10.0, 10.0));
        d.dispatch(&scene, up(10.0, 10.0));
        assert_eq!(*clicks.borrow(), 1);
    }
    #[test]
    fn callbacks_may_mutate_scene_and_keyboard_requires_press() {
        let (scene, a, _) = setup();
        let d = InputDispatcher::new();
        let hits = Rc::new(RefCell::new(0));
        let h = hits.clone();
        let s = scene.clone();
        let _a = d.register(
            a,
            NodeInput {
                focusable: true,
                ..Default::default()
            },
            move |e| {
                if e.event == InputEvent::Activate {
                    *h.borrow_mut() += 1;
                    s.borrow_mut()
                        .set_kind(a, NodeKind::Rect(Color(255, 0, 0, 255)));
                }
            },
        );
        d.focus(&scene, Some(a));
        let release = InputEvent::KeyUp {
            key: Key::Enter,
            modifiers: Modifiers::default(),
        };
        d.dispatch(&scene, release.clone());
        assert_eq!(*hits.borrow(), 0);
        d.dispatch(
            &scene,
            InputEvent::KeyDown {
                key: Key::Enter,
                modifiers: Modifiers::default(),
                repeat: false,
            },
        );
        d.dispatch(&scene, release);
        assert_eq!(*hits.borrow(), 1);
    }
}

#[cfg(test)]
mod listener_tests {
    use super::*;
    use crate::scene::{Color, NodeKind, Style};
    fn scene() -> (Rc<RefCell<Scene>>, NodeId) {
        let mut s = Scene::new(100., 100.);
        let node = s.append(
            s.root(),
            NodeKind::Rect(Color(1, 2, 3, 255)),
            Style {
                width: Some(50.),
                height: Some(50.),
                ..Default::default()
            },
        );
        s.flush();
        (Rc::new(RefCell::new(s)), node)
    }
    #[test]
    fn additive_bindings_drop_only_their_own_callback() {
        let (scene, node) = scene();
        let d = InputDispatcher::new();
        let log = Rc::new(RefCell::new(Vec::new()));
        let l = log.clone();
        let primary = d.register(
            node,
            NodeInput {
                focusable: true,
                ..Default::default()
            },
            move |_| l.borrow_mut().push(1),
        );
        let l = log.clone();
        let extra = d.listen(node, move |_| l.borrow_mut().push(2));
        d.dispatch_to(&scene, node, InputEvent::Activate);
        assert_eq!(*log.borrow(), vec![1, 2]);
        drop(extra);
        log.borrow_mut().clear();
        d.dispatch_to(&scene, node, InputEvent::Activate);
        assert_eq!(*log.borrow(), vec![1]);
        assert!(d.options(node).unwrap().focusable);
        drop(primary);
        assert!(!d.has_listeners(node));
    }
    #[test]
    fn stopping_propagation_preserves_same_node_listeners_and_default_action() {
        let (scene, node) = scene();
        let root = scene.borrow().root();
        let d = InputDispatcher::new();
        let log = Rc::new(RefCell::new(Vec::new()));
        let l = log.clone();
        let _root = d.register(root, NodeInput::default(), move |e| {
            if matches!(e.event, InputEvent::PointerDown { .. }) {
                l.borrow_mut().push(e.phase);
            }
        });
        let l = log.clone();
        let _first = d.register(
            node,
            NodeInput {
                focusable: true,
                ..Default::default()
            },
            move |e| {
                if matches!(e.event, InputEvent::PointerDown { .. }) {
                    l.borrow_mut().push(e.phase);
                    e.stop_propagation();
                }
            },
        );
        let l = log.clone();
        let extra = d.listen(node, move |e| {
            if matches!(e.event, InputEvent::PointerDown { .. }) {
                l.borrow_mut().push(e.phase);
            }
        });
        let down = InputEvent::PointerDown {
            x: 10.,
            y: 10.,
            button: PointerButton::Primary,
        };
        let result = d.dispatch(&scene, down.clone());
        assert!(!result.default_prevented);
        assert_eq!(d.focused(), Some(node));
        assert_eq!(
            *log.borrow(),
            vec![EventPhase::Capture, EventPhase::Target, EventPhase::Target]
        );
        drop(extra);
        d.focus(&scene, None);
        let _prevent = d.listen(node, |e| e.prevent_default());
        let result = d.dispatch(&scene, down);
        assert!(result.default_prevented);
        assert_eq!(d.focused(), None);
    }
    #[test]
    fn listeners_added_during_capture_wait_until_next_event_and_removed_are_skipped() {
        let (scene, node) = scene();
        let root = scene.borrow().root();
        let d = InputDispatcher::new();
        let log = Rc::new(RefCell::new(Vec::new()));
        let keep = Rc::new(RefCell::new(Vec::new()));
        let remove = Rc::new(RefCell::new(None));
        let l = log.clone();
        *remove.borrow_mut() = Some(d.register(node, NodeInput::default(), move |_| {
            l.borrow_mut().push("removed")
        }));
        let bindings = keep.clone();
        let remove = remove.clone();
        let dispatch = d.clone();
        let l = log.clone();
        let mut added = false;
        let _root = d.register(root, NodeInput::default(), move |e| {
            if e.phase == EventPhase::Capture && !added {
                added = true;
                remove.borrow_mut().take();
                let l = l.clone();
                bindings
                    .borrow_mut()
                    .push(dispatch.listen(node, move |_| l.borrow_mut().push("new")));
            }
        });
        d.dispatch_to(&scene, node, InputEvent::Activate);
        assert!(log.borrow().is_empty());
        d.dispatch_to(&scene, node, InputEvent::Activate);
        assert_eq!(*log.borrow(), vec!["new"]);
    }
    #[test]
    fn immediate_stop_skips_later_listeners_but_registration_replacement_is_safe() {
        let (scene, node) = scene();
        let d = InputDispatcher::new();
        let old = d.register(node, NodeInput::default(), |_| panic!("replaced callback"));
        let current = d.register(node, NodeInput::default(), |e| {
            e.stop_immediate_propagation()
        });
        drop(old);
        let _extra = d.listen(node, |_| panic!("immediately stopped callback"));
        d.dispatch_to(&scene, node, InputEvent::Activate);
        drop(current);
    }
}

#[cfg(test)]
mod reentrant_drop_tests {
    use super::*;
    use crate::scene::Scene;
    use std::cell::Cell;

    struct OnDrop(Option<Box<dyn FnOnce()>>);
    impl Drop for OnDrop {
        fn drop(&mut self) {
            self.0.take().unwrap()();
        }
    }

    #[test]
    fn callback_capture_drop_can_reenter_after_unregister_binding_drop_or_replace() {
        for action in 0..3 {
            let scene = Scene::new(100., 100.);
            let node = scene.root();
            let dispatcher = InputDispatcher::new();
            let reentrant = dispatcher.clone();
            let calls = Rc::new(Cell::new(0));
            let called = calls.clone();
            let resource = OnDrop(Some(Box::new(move || {
                // Replacement must already be visible; removal must already finish.
                assert_eq!(reentrant.has_listeners(node), action == 2);
                reentrant.unregister(node);
                reentrant.set_options(
                    node,
                    NodeInput {
                        disabled: true,
                        ..Default::default()
                    },
                );
                called.set(called.get() + 1);
            })));
            let binding = dispatcher.register(node, NodeInput::default(), move |_| {
                let _ = &resource;
            });
            match action {
                0 => {
                    dispatcher.unregister(node);
                    drop(binding);
                }
                1 => drop(binding),
                _ => {
                    let replacement = dispatcher.register(node, NodeInput::default(), |_| {});
                    drop(binding);
                    drop(replacement);
                }
            }
            assert_eq!(calls.get(), 1);
            assert!(!dispatcher.has_listeners(node));
            assert!(dispatcher.options(node).unwrap().disabled);
        }
    }

    #[test]
    fn dropping_listener_capture_can_drop_another_binding() {
        let scene = Scene::new(100., 100.);
        let node = scene.root();
        let dispatcher = InputDispatcher::new();
        let nested = dispatcher.listen(node, |_| {});
        let outer = dispatcher.listen(node, move |_| {
            let _ = &nested;
        });
        drop(outer);
        assert!(!dispatcher.has_listeners(node));
        assert!(dispatcher.options(node).is_none());
    }
}

#[cfg(test)]
mod disabled_observer_tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn disabled_observer_tracks_registration_metadata_and_detaches_on_removal() {
        let runtime = Runtime::new();
        let scene = Scene::new(100., 100.);
        let node = scene.root();
        let input = InputDispatcher::new();
        let disabled = input.observe_disabled(node, &runtime);
        let reads = Rc::new(Cell::new(0));
        let count = reads.clone();
        let read_disabled = disabled.clone();
        let _effect = runtime.effect(move || {
            read_disabled.get();
            count.set(count.get() + 1);
        });
        let binding = input.register(
            node,
            NodeInput {
                disabled: true,
                ..Default::default()
            },
            |_| {},
        );
        assert!(disabled.get());
        assert_eq!(reads.get(), 2);
        input.set_options(
            node,
            NodeInput {
                disabled: true,
                focusable: true,
                ..Default::default()
            },
        );
        assert_eq!(
            reads.get(),
            2,
            "unrelated/equal option updates do not notify"
        );
        drop(binding);
        assert!(!disabled.get());
        assert_eq!(reads.get(), 3);
        input.set_options(
            node,
            NodeInput {
                disabled: true,
                ..Default::default()
            },
        );
        assert!(disabled.get());
        input.unregister(node);
        assert!(!input.state.borrow().disabled_observers.contains_key(&node));
        let before = reads.get();
        input.set_options(node, NodeInput::default());
        assert_eq!(
            reads.get(),
            before,
            "removed observer cannot track reused metadata"
        );
        assert!(!input.observe_disabled(node, &runtime).get());
    }

    #[test]
    fn disabled_observer_callbacks_can_reenter_metadata_and_unregister() {
        let runtime = Runtime::new();
        let scene = Scene::new(100., 100.);
        let node = scene.root();
        let input = InputDispatcher::new();
        let disabled = input.observe_disabled(node, &runtime);
        let callback_input = input.clone();
        let _effect = runtime.effect(move || {
            if disabled.get() {
                assert!(callback_input.options(node).unwrap().disabled);
                callback_input.set_options(node, NodeInput::default());
                callback_input.unregister(node);
            }
        });
        let _binding = input.register(
            node,
            NodeInput {
                disabled: true,
                ..Default::default()
            },
            |_| {},
        );
        assert!(!input.has_listeners(node));
        assert!(input.options(node).is_none());
        assert!(input.state.borrow().disabled_observers.is_empty());
    }
}

#[path = "input_drag.rs"]
mod drag;
pub use drag::{DragEvent, DragPhase};
