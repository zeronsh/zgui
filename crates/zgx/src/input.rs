//! Retained event routing. Bindings disconnect when dropped; callbacks run without a scene borrow.
use crate::{
    reactive::{Runtime, Signal},
    scene::{NodeId, Scene},
};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
    time::{Duration, Instant},
};

/// The modifier keys held during an event.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    /// Shift.
    pub shift: bool,
    /// Control.
    pub control: bool,
    /// Alt, or Option on macOS.
    pub alt: bool,
    /// Command on macOS, the Windows or Super key elsewhere.
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
/// A key, by what it means rather than where it is (see
/// `EventContext::key_code` for the physical key).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Key {
    /// Tab.
    Tab,
    /// Enter or Return.
    Enter,
    /// The space bar.
    Space,
    /// Escape.
    Escape,
    /// Backspace: deletes before the caret.
    Backspace,
    /// Delete: deletes after the caret.
    Delete,
    /// Left arrow.
    ArrowLeft,
    /// Right arrow.
    ArrowRight,
    /// Up arrow.
    ArrowUp,
    /// Down arrow.
    ArrowDown,
    /// Home.
    Home,
    /// End.
    End,
    /// Page Up.
    PageUp,
    /// Page Down.
    PageDown,
    /// Insert.
    Insert,
    /// The Menu (Application) key: opens the focused view's context menu.
    ContextMenu,
    /// Function key number (native host supplies 1–35).
    Function(u8),
    /// A key that types a character, as the layout makes it (`"a"`, `"A"`).
    Character(String),
}
/// A pointer button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerButton {
    /// The main button: a mouse's left, a trackpad's click.
    Primary,
    /// The context button: a mouse's right, a two-finger click.
    Secondary,
    /// The middle button or wheel press.
    Middle,
    /// Another button, by the platform's number.
    Other(u16),
}
/// Complete native file transfers are rejected instead of truncated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileDropError {
    /// More files than one drop accepts.
    TooManyFiles,
    /// The files' data is larger than one drop accepts.
    TooLarge,
    /// The platform's data isn't a valid file list.
    InvalidData,
    /// The transfer didn't finish in time.
    TimedOut,
}
/// A trackpad scroll gesture's phase (`InputEvent::ScrollGesture`): the
/// fingers touching and lifting, then the momentum that may follow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollGesture {
    /// The fingers touched the trackpad.
    Began,
    /// The fingers lifted.
    Ended,
    /// Momentum scrolling began after the fingers lifted.
    MomentumBegan,
    /// Momentum scrolling ended.
    MomentumEnded,
}
/// A step back or forward through where an app has been, as a browser's
/// Back and Forward go: a trackpad's swipe between pages (macOS) reaches
/// the view under it as this action, in an [`InputEvent::Action`]
/// ([`InputDispatcher::navigate`]); a view takes it with
/// `View::on_navigate`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Navigation {
    /// Back, to where it was.
    Back,
    /// Forward again, to where it went back from.
    Forward,
}
/// An input event, routed to nodes by the [`InputDispatcher`]. Positions
/// are in window points from the top left.
#[derive(Clone, Debug, PartialEq)]
pub enum InputEvent {
    /// A typed action, from a key binding or a menu (see [`crate::actions`]).
    Action(crate::actions::Action),
    /// A drag between nodes, at one of its phases.
    Drag(DragEvent),
    /// A file dragged from outside the window is over `x`, `y`.
    FileHover {
        /// Across the window.
        x: f32,
        /// Down the window.
        y: f32,
        /// The file.
        path: std::path::PathBuf,
    },
    /// A file dragged from outside was dropped at `x`, `y`.
    FileDrop {
        /// Across the window.
        x: f32,
        /// Down the window.
        y: f32,
        /// The file.
        path: std::path::PathBuf,
    },
    /// A file drag from outside left the window or was cancelled.
    FileHoverCancelled,
    /// Files dropped at `x`, `y` were rejected whole, for `reason`.
    FilesDropRejected {
        /// Across the window.
        x: f32,
        /// Down the window.
        y: f32,
        /// Why they were rejected.
        reason: FileDropError,
    },
    /// Paths delivered together by one native event-loop dispatch batch.
    FilesDrop {
        /// Across the window.
        x: f32,
        /// Down the window.
        y: f32,
        /// The files, in the order the platform gave them.
        paths: std::sync::Arc<[std::path::PathBuf]>,
    },
    /// A pointer button went down at `x`, `y`.
    PointerDown {
        /// Across the window.
        x: f32,
        /// Down the window.
        y: f32,
        /// Which button.
        button: PointerButton,
    },
    /// A pointer button went up at `x`, `y`.
    PointerUp {
        /// Across the window.
        x: f32,
        /// Down the window.
        y: f32,
        /// Which button.
        button: PointerButton,
    },
    /// The pointer moved to `x`, `y`.
    PointerMove {
        /// Across the window.
        x: f32,
        /// Down the window.
        y: f32,
    },
    /// The pointer left the node (or the window).
    PointerLeave,
    /// The platform took the pointer away mid-gesture: a press ends without
    /// activating.
    PointerCancel,
    /// The pointer entered the node.
    PointerEnter,
    /// A wheel or trackpad scroll at `x`, `y`.
    Scroll {
        /// Across the window.
        x: f32,
        /// Down the window.
        y: f32,
        /// Points to scroll across; positive moves content left.
        delta_x: f32,
        /// Points to scroll down; positive moves content up.
        delta_y: f32,
    },
    /// A trackpad pinch at `x`, `y`: the fingers' spread changed by `delta`,
    /// a fraction of it (0.05 apart is 5% larger: zoom in; negative, out).
    /// It goes to the node under the pointer, as a scroll does.
    Pinch {
        /// Across the window.
        x: f32,
        /// Down the window.
        y: f32,
        /// The change in scale.
        delta: f32,
    },
    /// A trackpad scroll gesture's phase, which the host reports around its
    /// `Scroll`s: what elastic scrolling springs back on
    /// (`Ui::set_elastic_scrolling`). A mouse wheel has none. The document
    /// takes it; it goes to no node.
    ScrollGesture(ScrollGesture),
    /// A key went down.
    KeyDown {
        /// The key.
        key: Key,
        /// The modifiers held.
        modifiers: Modifiers,
        /// Whether the key is held and repeating.
        repeat: bool,
    },
    /// A key came up.
    KeyUp {
        /// The key.
        key: Key,
        /// The modifiers held.
        modifiers: Modifiers,
    },
    /// Text typed, after the keyboard layout and dead keys.
    Text(String),
    /// An input method's composition in progress, shown in the editor
    /// until it commits.
    ImePreedit {
        /// The composition so far.
        text: String,
        /// The input method's caret or selection in `text`, as UTF-8 byte
        /// offsets.
        cursor: Option<(usize, usize)>,
    },
    /// An input method committed this text.
    ImeCommit(String),
    /// The node took keyboard focus.
    Focus,
    /// The modal scope rooted at the node closed.
    FocusScopeClosed,
    /// The node lost keyboard focus.
    Blur,
    /// The node was activated: clicked, or Enter or Space while focused.
    Activate,
    /// Assistive technology asks to set an editor's text.
    SetValue(String),
    /// Set committed-text selection using UTF-8 byte offsets.
    SetTextSelection {
        /// Where the selection starts.
        anchor: usize,
        /// Where it ends, at the caret.
        focus: usize,
    },
    /// Assistive technology asks to set a slider's value.
    SetNumericValue(f64),
    /// Assistive technology asks a slider to step up.
    Increment,
    /// Assistive technology asks a slider to step down.
    Decrement,
}
/// Which part of its route an event is in, as the DOM's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventPhase {
    /// Going down from the root to the target.
    Capture,
    /// At the target.
    Target,
    /// Going back up from the target to the root.
    Bubble,
}
/// Source, payload and optional window press position of a requested drag.
type DragRequest = (NodeId, crate::actions::Action, Option<(f32, f32)>);
/// An event on its route, as a listener gets it: what it is, where it is
/// going, and what listeners so far decided.
pub struct EventContext {
    /// The event.
    pub event: InputEvent,
    /// The node it is routed to.
    pub target: NodeId,
    /// The node whose listener is running.
    pub current_target: NodeId,
    /// Which part of the route it is in.
    pub phase: EventPhase,
    stopped: bool,
    immediate: bool,
    prevented: bool,
    focus: Option<NodeId>,
    capture: Option<Option<NodeId>>,
    pointer_modifiers: Modifiers,
    click_count: u8,
    drag_threshold: f32,
    drag_request: Option<DragRequest>,
    drag_accepted: Option<NodeId>,
    /// What a starting drag carries to other apps (`set_drag_export`).
    drag_export: Option<Rc<[DragItem]>>,
    /// A starting drag moves its window (`set_drag_moves_window`).
    drag_moves_window: bool,
    key_code: Option<Rc<str>>,
    /// `take_pointer`'s taker: what the pointer was pressed on gives it up.
    take_pointer: Option<NodeId>,
    /// The node whose listener first prevented the default: a scroll view
    /// that took a wheel gesture, which latches to it.
    prevented_by: Option<NodeId>,
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
            drag_threshold: DRAG_THRESHOLD,
            drag_request: None,
            drag_accepted: None,
            drag_export: None,
            drag_moves_window: false,
            key_code: None,
            take_pointer: None,
            prevented_by: None,
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
    /// On a drag's Start, at its source: what it carries to other apps
    /// (`View::drag_export`).
    pub fn set_drag_export(&mut self, items: Vec<DragItem>) {
        if matches!(&self.event, InputEvent::Drag(event) if event.phase == DragPhase::Start) {
            self.drag_export = Some(items.into());
        }
    }
    /// On a drag's Start, at its source: the drag moves its window rather
    /// than showing a preview (`View::drag_moves_window`).
    pub fn set_drag_moves_window(&mut self) {
        if matches!(&self.event, InputEvent::Drag(event) if event.phase == DragPhase::Start) {
            self.drag_moves_window = true;
        }
    }
    /// Accept the drag over this node, so dropping it here delivers the drop.
    /// Only while the drag is over, and not in the capture phase.
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
    /// Primary or other button press count: 1, 2 for a double click's
    /// second, 3 for a triple's and on past it, as the web's `detail` counts;
    /// zero for non-press events.
    pub fn click_count(&self) -> u8 {
        self.click_count
    }
    /// How far, in points, the pointer moves from a press before it is a
    /// drag rather than a click (`InputDispatcher::set_drag_threshold`).
    pub fn drag_threshold(&self) -> f32 {
        self.drag_threshold
    }
    /// A key event's physical key, as the DOM's `KeyboardEvent.code`
    /// (`"KeyA"`, `"Digit1"`, `"BracketLeft"`): the same key whatever the
    /// layout or modifiers made it type. None when the platform didn't say.
    pub fn key_code(&self) -> Option<&str> {
        self.key_code.as_deref()
    }
    /// Stop the event at this node: no ancestor's listener hears it.
    pub fn stop_propagation(&mut self) {
        self.stopped = true;
    }
    /// Stop remaining listeners on this node as well as propagation to ancestors.
    pub fn stop_immediate_propagation(&mut self) {
        self.stopped = true;
        self.immediate = true;
    }
    /// Prevent the built-in handling (a button's activation, an editor's
    /// edit) and tell ancestors it was handled.
    pub fn prevent_default(&mut self) {
        self.prevented = true;
        self.prevented_by.get_or_insert(self.current_target);
    }
    /// A scroll view moved along its axis (`horizontal` or not) for this
    /// wheel event: the event is used, unless it also moves along the other
    /// axis, which goes on to the views around it (a vertical swipe over a
    /// horizontal carousel still scrolls the page).
    pub(crate) fn scrolled(&mut self, horizontal: bool) {
        if let InputEvent::Scroll {
            delta_x, delta_y, ..
        } = &mut self.event
        {
            let (own, other) = if horizontal {
                (delta_x, delta_y)
            } else {
                (delta_y, delta_x)
            };
            if *other != 0. && other.is_finite() {
                *own = 0.;
                return;
            }
        }
        self.prevent_default();
        self.stop_propagation();
    }
    /// Focus this node once the event is delivered.
    pub fn focus(&mut self) {
        self.focus = Some(self.current_target);
    }
    /// Whether a listener so far asked to capture or release the pointer.
    pub(crate) fn capture_requested(&self) -> bool {
        self.capture.is_some()
    }
    /// What a listener so far asked capture for: `Some(None)` a release.
    pub(crate) fn capture_target(&self) -> Option<Option<NodeId>> {
        self.capture
    }
    /// Capture pointer routing to this node until the initiating button is
    /// released or the gesture is cancelled. Captures outside pointer-down
    /// retain the current capture button, defaulting to the primary button
    /// when none exists.
    pub fn capture_pointer(&mut self) {
        self.capture = Some(Some(self.current_target));
    }
    /// Release a pointer capture.
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
    /// Whether a listener so far has prevented the default.
    pub fn default_prevented(&self) -> bool {
        self.prevented
    }
}
/// How long a wheel gesture stays latched to its scroll view after its
/// last event: longer than the gaps between a gesture's and its momentum's
/// events, shorter than a person's pause between gestures.
const SCROLL_LATCH: Duration = Duration::from_millis(300);
/// How a node takes input.
#[derive(Clone, Copy, Debug, Default)]
pub struct NodeInput {
    /// Whether it can take keyboard focus.
    pub focusable: bool,
    /// CSS `pointer-events: none`: the pointer passes through the node, and
    /// through descendants that have no listeners of their own, to what is
    /// under it.
    pub pass_through: bool,
    /// Whether it is disabled: no input reaches it or its subtree, and it
    /// can't take focus.
    pub disabled: bool,
    /// Its place in Tab order, as HTML's `tabindex`: positive indices first,
    /// in order, then 0s in document order; negative leaves Tab order.
    pub tab_index: i32,
}
type Callback = Rc<RefCell<dyn FnMut(&mut EventContext)>>;
struct Entry {
    token: u64,
    callback: Callback,
}
/// The arrow keys that move focus within a focus group
/// (`InputDispatcher::set_focus_group`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusGroup {
    /// Up and Down: a list box, a menu.
    Vertical,
    /// Left and Right: a toolbar, tabs.
    Horizontal,
    /// All four: a grid of choices.
    Both,
}
/// A focus group: its arrows, and the member Tab lands on.
#[derive(Clone, Copy)]
struct Group {
    /// None: one Tab stop whose arrows its owner handles (a virtual list).
    axis: Option<FocusGroup>,
    current: Option<NodeId>,
    /// Its arrows choose what they move to, wrapping at the ends: a radio
    /// group's (`set_group_chooses`).
    chooses: bool,
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
/// The last press, how soon after it another counts as its second or third
/// (the system's double-click time), and how far from it the next may land:
/// half the system's double-click box, where the host gives one.
struct ClickTracker(Option<Click>, Duration, Option<(f32, f32)>);
/// Windows' and macOS's default double-click time, until the host gives the
/// system's.
const DOUBLE_CLICK: Duration = Duration::from_millis(500);
/// How far the pointer moves from a press before it is a drag, until the
/// host gives the system's: Windows' default (`SM_CXDRAG`).
const DRAG_THRESHOLD: f32 = 4.;
impl Default for ClickTracker {
    fn default() -> Self {
        Self(None, DOUBLE_CLICK, None)
    }
}
impl ClickTracker {
    fn moved(&mut self, x: f32, y: f32) {
        if self
            .0
            .is_some_and(|click| !self.near(click.position, (x, y)))
        {
            self.0 = None;
        }
    }
    fn near(&self, a: (f32, f32), b: (f32, f32)) -> bool {
        let dx = f64::from(a.0) - f64::from(b.0);
        let dy = f64::from(a.1) - f64::from(b.1);
        match self.2 {
            Some((x, y)) => dx.abs() <= f64::from(x) && dy.abs() <= f64::from(y),
            None => dx * dx + dy * dy <= 16.,
        }
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
                        .is_some_and(|elapsed| elapsed <= self.1)
                    && self.near(old.position, position)
            })
            .map_or(1, |old| old.count.saturating_add(1));
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
    /// Views that take keys while nothing is focused (`take_unfocused_keys`),
    /// the last mounted first.
    unfocused_keys: Vec<NodeId>,
    /// Views that take a step back or forward (`take_navigation`).
    navigators: Vec<NodeId>,
    entries: crate::scene::NodeMap<Vec<Entry>>,
    options: crate::scene::NodeMap<NodeInput>,
    /// Boxes the pointer passes through while nothing they hold is displayed
    /// (`View::hollow`): a presence's box over a page kept mounted but hidden.
    /// A few at most: a list, not a map (a map's instance shared with another
    /// set's outlined that one's insert).
    hollow: Vec<NodeId>,
    disabled_observers: crate::scene::NodeMap<Signal<bool>>,
    next: u64,
    focused: Option<NodeId>,
    captured: Option<NodeId>,
    capture_button: Option<PointerButton>,
    /// The hovered node (the pointer's hit target) and its ancestors, up to
    /// the active modal root.
    hover: Vec<NodeId>,
    /// The nodes told of hover, outermost first: each has had its enter,
    /// none its leave yet. `set_hover` tells them until they are `hover`,
    /// a step at a time, so a listener's own dispatch takes them on from
    /// where they are.
    hover_told: Vec<NodeId>,
    /// Where the pointer last was over the window (None: outside it).
    pointer: Option<(f32, f32)>,
    file_hovered: Option<NodeId>,
    pressed: Option<NodeId>,
    key_pressed: Option<(NodeId, Key)>,
    /// The window's semantics: a control's role says which keys activate it
    /// (`activates`).
    semantics: Weak<RefCell<crate::semantics::Semantics>>,
    scopes: Vec<FocusScope>,
    /// Focus groups (`set_focus_group`): one Tab stop each.
    groups: crate::scene::NodeMap<Group>,
    /// Non-modal layers (toasts): what they hold stays live under any modal
    /// scope (`set_layer`).
    layers: Vec<NodeId>,
    /// Scope roots and their triggers, which stay live to the pointer while
    /// the scope is active (`set_scope_trigger`).
    triggers: Vec<(NodeId, NodeId)>,
    /// The press being dispatched dismissed a popover on blank space
    /// (`keep_focus_for_press`): it leaves focus where dismissing put it.
    keep_focus: bool,
    clicks: ClickTracker,
    /// The system's drag threshold, once the host gives it.
    drag_threshold: Option<f32>,
    drag: Option<drag::DragSession>,
    /// Listeners of the drag's source removed while it went on (a drop that
    /// moves the dragged thing elsewhere): they still hear the drag end.
    drag_orphans: Vec<Callback>,
    /// The source of a drag being finished (its drop and end delivering).
    drag_ending: Option<NodeId>,
    /// A drag from another window over this one (`drag_over`).
    visit: Option<drag::Visit>,
    /// How the last drag ended, until its host takes it (`take_drag_ended`).
    drag_ended: Option<drag::DragEnded>,
    /// The destination the drag rests on, toward springing it open.
    spring: Option<drag::SpringArm>,
    /// The spring-loading delay the host set (`set_spring_delay`): `None`
    /// until it does, `Some(None)` where it turned spring-loading off.
    spring_delay: Option<Option<std::time::Duration>>,
    /// The physical key of the key event being dispatched (`set_key_code`).
    key_code: Option<Rc<str>>,
    /// The scroll view a wheel gesture is latched to, the axis it moves
    /// along (horizontal or not) and when it last scrolled: the rest of the
    /// gesture along that axis goes to it alone.
    scroll_latch: Option<(NodeId, bool, Instant)>,
    /// The modifier keys last known held (from the platform's pointer and
    /// key events): what an activation (a click) was made with.
    modifiers: Modifiers,
    /// The last interaction was the pointer (a press), not the keyboard:
    /// what it focuses shows no focus ring (`focus_visible_now`).
    pointer_modality: bool,
}
/// Routes input events to the nodes listening for them, and holds focus,
/// pointer capture, modal scopes and key bindings. Clones share one
/// state.
#[derive(Clone, Default)]
pub struct InputDispatcher {
    state: Rc<RefCell<State>>,
    actions: Rc<RefCell<crate::actions::Registry>>,
}
/// A listener on a node; dropping it removes the listener.
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
                // A hollow node has options: only one removed can be.
                if s.options.remove(&self.node).is_some() && !s.hollow.is_empty() {
                    s.hollow.retain(|node| *node != self.node);
                }
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
/// What dispatching an event did.
#[derive(Clone, Copy, Debug, Default)]
pub struct DispatchResult {
    /// The node it was routed to, if any.
    pub target: Option<NodeId>,
    /// Whether a listener prevented its default.
    pub default_prevented: bool,
    /// Whether focus moved.
    pub focus_changed: bool,
}
impl InputDispatcher {
    /// Keys go to `node` while nothing is focused (and no modal is open),
    /// as a page's go to its body: an app's shortcuts reach its root's
    /// listeners. The last such node still in the scene takes them.
    pub fn take_unfocused_keys(&self, node: NodeId) {
        let mut state = self.state.borrow_mut();
        state.unfocused_keys.retain(|n| *n != node);
        state.unfocused_keys.push(node);
    }
    /// `node` takes a step back or forward ([`Navigation`]) from what is
    /// under it: a trackpad's swipe over it navigates rather than scrolls
    /// where nothing between scrolls that way (`Ui::set_navigation_swipes`).
    /// It hears the action with its listeners (`View::on_navigate`).
    pub fn take_navigation(&self, node: NodeId, scene: &Scene) {
        let mut state = self.state.borrow_mut();
        state
            .navigators
            .retain(|n| *n != node && scene.contains(*n));
        state.navigators.push(node);
    }
    /// Whether `target` or a view around it takes navigation
    /// (`take_navigation`). Reads only.
    pub(crate) fn navigates(&self, scene: &Scene, target: NodeId) -> bool {
        let state = self.state.borrow();
        !state.navigators.is_empty()
            && std::iter::once(target)
                .chain(scene.ancestors(target))
                .any(|node| state.navigators.contains(&node))
    }
    /// Step back or forward from what is under `x`, `y`, as a swipe there
    /// does: the view under it hears `navigation`, and the views around it
    /// as it bubbles. Whether one took it.
    pub fn navigate(
        &self,
        scene: &Rc<RefCell<Scene>>,
        x: f32,
        y: f32,
        navigation: Navigation,
    ) -> bool {
        let Some(target) = self.hit_target(&scene.borrow(), x, y) else {
            return false;
        };
        self.dispatch_to(
            scene,
            target,
            InputEvent::Action(crate::actions::Action::new(navigation)),
        )
        .default_prevented
    }
    /// The next press starts a count of its own, as one after the
    /// double-click time has passed would: separate clicks a test makes in
    /// a row, which a person's hand could not make that fast.
    pub fn forget_clicks(&self) {
        self.state.borrow_mut().clicks.0 = None;
    }
    /// How soon after a press another on the same spot counts as a double
    /// (then triple) click: the system's double-click time, which the
    /// desktop host gives (macOS's and Windows' setting, GNOME's and KDE's);
    /// 500 ms until it does. Held between 100 ms and 5 s.
    pub fn set_double_click_time(&self, interval: Duration) {
        self.state.borrow_mut().clicks.1 =
            interval.clamp(Duration::from_millis(100), Duration::from_secs(5));
    }
    /// The box, in points, centred on a press, that the next must land in
    /// to count as its double (then triple) click: the system's, which the
    /// desktop host gives (Windows' `SM_CXDOUBLECLK` by `SM_CYDOUBLECLK`,
    /// which people whose hands shake widen); until it does, within 4
    /// points of the press in any direction. Each side held between 1 and
    /// 200; NaN leaves the box as it was.
    pub fn set_double_click_area(&self, width: f32, height: f32) {
        if width.is_nan() || height.is_nan() {
            return;
        }
        self.state.borrow_mut().clicks.2 =
            Some((width.clamp(1., 200.) / 2., height.clamp(1., 200.) / 2.));
    }
    /// The double-click box's width and height in points
    /// (`set_double_click_area`), once the host has given one.
    pub fn double_click_area(&self) -> Option<(f32, f32)> {
        let (x, y) = self.state.borrow().clicks.2?;
        Some((x * 2., y * 2.))
    }
    /// How far, in points, the pointer moves from a press before it is a
    /// drag rather than a click (`EventContext::drag_threshold`): the
    /// system's, which the desktop host gives (GNOME's and KDE's setting,
    /// Windows' `SM_CXDRAG`), and which people whose hands shake raise; 4
    /// until it does. Held
    /// between 1 and 100; NaN leaves it as it was.
    pub fn set_drag_threshold(&self, points: f32) {
        if points.is_nan() {
            return;
        }
        self.state.borrow_mut().drag_threshold = Some(points.clamp(1., 100.));
    }
    /// The drag threshold in points (`set_drag_threshold`).
    pub fn drag_threshold(&self) -> f32 {
        self.state.borrow().drag_threshold.unwrap_or(DRAG_THRESHOLD)
    }
    /// A dispatcher with no listeners and nothing focused.
    pub fn new() -> Self {
        Self::default()
    }
    /// The semantics whose roles say which keys activate a control.
    pub(crate) fn set_semantics(&self, semantics: &Rc<RefCell<crate::semantics::Semantics>>) {
        self.state.borrow_mut().semantics = Rc::downgrade(semantics);
    }
    /// Whether `key` activates `node`, as Chrome's and AppKit's controls
    /// take it: Enter as it goes down, Space as it comes up, except that a
    /// link takes Enter alone and a checkbox, radio button or switch Space
    /// alone (Enter goes on to the form's default action).
    pub(crate) fn activates(&self, node: NodeId, key: &Key) -> bool {
        use crate::semantics::Role;
        let semantics = self.state.borrow().semantics.upgrade();
        let role = semantics.and_then(|s| s.borrow().get(node).map(|node| node.role));
        match key {
            Key::Enter => !matches!(
                role,
                Some(Role::CheckBox | Role::RadioButton | Role::Switch)
            ),
            Key::Space => role != Some(Role::Link),
            _ => false,
        }
    }
    /// Register a scoped keymap. Keep its guard alive for the component lifetime.
    pub fn bind_keys(
        &self,
        node: NodeId,
        map: crate::actions::Keymap,
    ) -> crate::actions::KeymapBinding {
        crate::actions::Registry::bind(&self.actions, node, map)
    }
    /// Whether a key binding where focus is, or above it, takes `key` with
    /// `modifiers` (alone or as a sequence's first stroke). A host asks
    /// before taking a key from a native view that would otherwise use it
    /// (a terminal's): one zgx binds is the app's shortcut. Reads only.
    pub fn binds_key(&self, scene: &Scene, key: &Key, modifiers: Modifiers) -> bool {
        let path = self.action_path(scene);
        let stroke = crate::actions::Keystroke::new(key.clone(), modifiers);
        let code = self.state.borrow().key_code.clone();
        self.actions
            .try_borrow()
            .is_ok_and(|actions| actions.binds(&path, &stroke, code.as_deref()))
    }
    /// Whether the key binding that takes `key` with `modifiers` where focus
    /// is (`binds_key`) is reserved (`KeyBinding::reserved`): the app's
    /// before a native view (a page, a terminal) has it. Reads only.
    pub fn reserves_key(&self, scene: &Scene, key: &Key, modifiers: Modifiers) -> bool {
        let path = self.action_path(scene);
        let stroke = crate::actions::Keystroke::new(key.clone(), modifiers);
        let code = self.state.borrow().key_code.clone();
        self.actions
            .try_borrow()
            .is_ok_and(|actions| actions.reserves(&path, &stroke, code.as_deref()))
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
    /// Whether a multi-key sequence is waiting for its next key.
    pub fn has_pending_keys(&self) -> bool {
        self.next_key_deadline().is_some()
    }
    /// Drop the keys of a sequence in progress.
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
    /// Whether `node` has a listener.
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
    /// How `node` takes input, if it is registered.
    pub fn options(&self, node: NodeId) -> Option<NodeInput> {
        self.state.borrow().options.get(&node).copied()
    }
    /// Mark `node` (registered: `set_options`) as hollow: the pointer passes
    /// through it while nothing it holds is displayed.
    pub(crate) fn set_hollow(&self, node: NodeId) {
        let mut state = self.state.borrow_mut();
        if !state.hollow.contains(&node) {
            state.hollow.push(node);
        }
    }
    /// Whether the pointer passes through `node`, which takes input as
    /// `options` say, to what is under it: `pass_through`, or hollow with
    /// nothing in it displayed (`Scene::displays_content`).
    pub(crate) fn passes(&self, scene: &Scene, node: NodeId, options: &NodeInput) -> bool {
        options.pass_through
            || self
                .state
                .try_borrow()
                .is_ok_and(|state| !state.hollow.is_empty() && state.hollow.contains(&node))
                && !scene.displays_content(node)
    }
    /// `options`, and whether `node` is hollow, in one look at the state:
    /// for the pointer's walk, which asks for every node it passes.
    fn options_hollow(&self, node: NodeId) -> (Option<NodeInput>, bool) {
        let state = self.state.borrow();
        let options = state.options.get(&node).copied();
        let hollow = options.is_some() && !state.hollow.is_empty() && state.hollow.contains(&node);
        (options, hollow)
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
        // A hollow node has options: only one removed can be.
        if s.options.remove(&node).is_some() && !s.hollow.is_empty() {
            s.hollow.retain(|hollow| *hollow != node);
        }
        s.groups.remove(&node);
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
    /// Unregister the nodes the scene no longer has that the pointer,
    /// focus, a press, a drag or a scope still names, as removing them does
    /// (`Ui::remove`, which leaves the others' registrations to be freed
    /// later: nothing routes to a node the scene no longer has, but through
    /// these). The scene is read only to find them: unregistering drops
    /// their listeners, whose destructors may remove nodes.
    pub(crate) fn forget_removed(&self, scene: &RefCell<Scene>) {
        let named: Vec<NodeId> = {
            let scene = scene.borrow();
            let s = self.state.borrow();
            s.focused
                .into_iter()
                .chain(s.captured)
                .chain(s.pressed)
                .chain(s.key_pressed.as_ref().map(|(id, _)| *id))
                .chain(s.clicks.0.map(|click| click.target))
                .chain(s.hover.iter().copied())
                .chain(s.hover_told.iter().copied())
                .chain(s.file_hovered)
                .chain(s.scroll_latch.map(|(id, ..)| id))
                .chain(s.drag.iter().flat_map(drag::DragSession::nodes))
                .chain(s.drag_ending)
                .chain(s.visit.as_ref().and_then(drag::Visit::node))
                .chain(
                    s.scopes
                        .iter()
                        .flat_map(|scope| [Some(scope.root), scope.previous])
                        .flatten(),
                )
                .chain(s.layers.iter().copied())
                .chain(s.unfocused_keys.iter().copied())
                .filter(|id| !scene.contains(*id))
                .collect()
        };
        for id in named {
            self.unregister(id);
        }
    }
    /// The node with keyboard focus, if any.
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
    /// What a press at `x, y` lands on, as dispatch finds it.
    pub(crate) fn pointer_target(&self, scene: &Scene, x: f32, y: f32) -> Option<NodeId> {
        self.hit_target(scene, x, y)
    }
    /// What a primary press at `x, y` would focus, as dispatch finds it:
    /// the nearest focusable, enabled node at or above what it lands on.
    pub(crate) fn press_focus(&self, scene: &Scene, x: f32, y: f32) -> Option<NodeId> {
        let mut node = self
            .hit_target(scene, x, y)
            .filter(|id| self.enabled(scene, *id));
        while let Some(id) = node {
            if self.options(id).is_some_and(|o| o.focusable) {
                return Some(id);
            }
            node = scene.parent(id);
        }
        None
    }
    /// Move hover to `hit` and its ancestors, as the DOM's `mouseenter` and
    /// `mouseleave` do: a node hears `PointerLeave` once the pointer is
    /// outside it and all its descendants, and `PointerEnter` once it comes
    /// inside; moving between descendants tells it nothing. Each node hears
    /// its own, at the target phase (they do not bubble): the nodes left,
    /// deepest first, then the nodes entered, outermost first. Costs a walk
    /// up from `hit` when the hit target changes, nothing when it does not.
    /// Hover nothing until the pointer next moves (a menu item flashing
    /// off under a still pointer, `compose_menu::flash`).
    pub(crate) fn clear_hover(&self, scene: &Rc<RefCell<Scene>>) {
        self.set_hover(scene, None);
    }
    /// Move hover to `hit` and its ancestors, telling those left and
    /// entered; whether any listener heard (and might have changed the
    /// scene).
    fn set_hover(&self, scene: &Rc<RefCell<Scene>>, hit: Option<NodeId>) -> bool {
        if self.state.borrow().hover.first().copied() == hit {
            return false;
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
        self.state.borrow_mut().hover = next;
        let mut heard = false;
        // Leave the innermost told node past where they agree, else enter
        // the next hovered one past it, and look again: a listener that
        // dispatched may have moved hover on. (Both are ancestor chains:
        // told from the outermost, hover from the hit.)
        loop {
            let step = {
                let mut state = self.state.borrow_mut();
                let state = &mut *state;
                let common = state
                    .hover_told
                    .iter()
                    .zip(state.hover.iter().rev())
                    .take_while(|(told, hovered)| told == hovered)
                    .count();
                if state.hover_told.len() > common {
                    state
                        .hover_told
                        .pop()
                        .map(|id| (id, InputEvent::PointerLeave))
                } else if state.hover.len() > common {
                    let id = state.hover[state.hover.len() - 1 - common];
                    state.hover_told.push(id);
                    Some((id, InputEvent::PointerEnter))
                } else {
                    None
                }
            };
            let Some((id, event)) = step else {
                return heard;
            };
            heard |= self.hover_event(scene, id, event);
        }
    }
    /// Deliver an enter or leave to `node`'s own listeners only, while it
    /// still holds (a listener's dispatch may have left or entered it
    /// again); whether it has any.
    fn hover_event(&self, scene: &Rc<RefCell<Scene>>, node: NodeId, event: InputEvent) -> bool {
        let entering = matches!(event, InputEvent::PointerEnter);
        if !scene.borrow().contains(node) {
            return false;
        }
        let callbacks: Vec<_> = {
            let state = self.state.borrow();
            let Some(entries) = state.entries.get(&node) else {
                return false;
            };
            // A disabled node is entered no more; leaving still cleans up.
            if matches!(event, InputEvent::PointerEnter)
                && state.options.get(&node).is_some_and(|o| o.disabled)
            {
                return false;
            }
            entries
                .iter()
                .map(|e| (e.token, e.callback.clone()))
                .collect()
        };
        let mut ctx = EventContext::new(event, node);
        for (token, callback) in callbacks {
            let (attached, current) = {
                let state = self.state.borrow();
                let attached = state
                    .entries
                    .get(&node)
                    .is_some_and(|entries| entries.iter().any(|entry| entry.token == token));
                (attached, state.hover_told.contains(&node) == entering)
            };
            // A listener before it moved hover: what follows was told anew.
            if !current {
                break;
            }
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
        true
    }
    /// The node a key went to, if it is still there and still focused: its
    /// listeners may have removed it or moved focus on (a dialog's button
    /// closing the dialog), and what has focus now did not hear the key.
    fn still_targeted(&self, scene: &Rc<RefCell<Scene>>, target: Option<NodeId>) -> Option<NodeId> {
        target.filter(|id| {
            let scene = scene.borrow();
            // Inert content (a row leaving) activates on no key.
            self.focused() == Some(*id) && scene.contains(*id) && scene.inert_around(*id).is_none()
        })
    }
    /// Move focus off inert content (`Scene::set_inert`: a row playing its
    /// exit, gone from its data): to the next focusable control after it,
    /// else the one before, else the nearest focusable around it, as a
    /// platform's list moves focus off a row it deletes. Whoever makes
    /// content inert calls it (`NodeHandle::leave`).
    pub fn leave_inert(&self, scene: &Rc<RefCell<Scene>>) {
        let target = {
            let scene = scene.borrow();
            let Some(focused) = self.focused() else {
                return;
            };
            let Some(leaving) = scene.inert_around(focused) else {
                return;
            };
            let takes = |id: NodeId| {
                self.options(id)
                    .is_some_and(|o| o.focusable && o.tab_index >= 0)
                    && scene.inert_around(id).is_none()
                    && self.enabled(&scene, id)
            };
            // Document order, leaving's subtree passed over.
            let (mut before, mut after) = (None, None);
            let mut passed = false;
            let mut stack = vec![scene.root()];
            while let Some(id) = stack.pop() {
                if id == leaving {
                    passed = true;
                    continue;
                }
                if takes(id) {
                    if passed {
                        after = Some(id);
                        break;
                    }
                    before = Some(id);
                }
                stack.extend(scene.children(id).iter().rev().copied());
            }
            after
                .or(before)
                .or_else(|| scene.ancestors(leaving).find(|id| takes(*id)))
        };
        if target.is_none() || !self.focus(scene, target) {
            self.focus(scene, None);
        }
    }
    /// Whether `node` can take input now: shown, not disabled itself or by
    /// an ancestor, not transparent, and inside the active modal scope.
    pub fn is_enabled(&self, scene: &Scene, node: NodeId) -> bool {
        self.enabled(scene, node)
    }
    /// The node capturing the pointer, if any.
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
    /// Make `root` a non-modal layer (toasts): what it holds takes the
    /// pointer and keys even while a modal holds focus elsewhere.
    pub fn set_layer(&self, root: NodeId, layer: bool) {
        let mut s = self.state.borrow_mut();
        s.layers.retain(|id| *id != root);
        if layer {
            s.layers.push(root);
        }
    }
    /// Keep `trigger` live while `root`'s scope is active, though outside it
    /// (a popover's trigger row in its parent's panel, which toggles it), or
    /// with `None`, no longer.
    pub(crate) fn set_scope_trigger(&self, root: NodeId, trigger: Option<NodeId>) {
        let mut s = self.state.borrow_mut();
        s.triggers.retain(|(scope, _)| *scope != root);
        if let Some(trigger) = trigger {
            s.triggers.push((root, trigger));
        }
    }
    /// The primary press about to be dispatched leaves focus as it is: a
    /// popover it dismissed on blank space gave focus back to its trigger.
    pub(crate) fn keep_focus_for_press(&self) {
        self.state.borrow_mut().keep_focus = true;
    }
    /// The active modal root, if any. Focus and pointer routing stay inside
    /// it.
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
        // Inside the active scope (or its trigger, `set_scope_trigger`), or,
        // through porous scopes (submenus), any scope below it.
        let mut live = s.scopes.iter().rev().peekable();
        if !in_layer && live.peek().is_some() {
            let mut inside = false;
            // Most scopes have no trigger (`set_scope_trigger`).
            let triggers = !s.triggers.is_empty();
            for scope in live {
                let trigger = triggers
                    .then(|| s.triggers.iter().find(|(root, _)| *root == scope.root))
                    .flatten()
                    .map(|(_, trigger)| *trigger);
                let mut ancestor = Some(node);
                while let Some(id) = ancestor {
                    if id == scope.root || Some(id) == trigger {
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
    /// Focus `target`, or clear focus with `None`. Returns false, changing
    /// nothing, if `target` can't take focus.
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
        // Its group's Tab stop is where focus last was in it.
        if let Some(id) = target {
            self.set_group_current(&scene.borrow(), id);
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
        // An activation carries the keys held as it was made (a ⌘-click),
        // and a drag's phases the keys held over it (Option to copy).
        let held = if matches!(event, InputEvent::Activate | InputEvent::Drag(_)) {
            self.state.borrow().modifiers
        } else {
            Modifiers::default()
        };
        self.route_pointer(scene, target, event, held, count, false)
    }
    /// Route `event` from `target` through its ancestors (capture, target,
    /// bubble), or to `target` `alone`: a latched wheel gesture's.
    fn route_pointer(
        &self,
        scene: &Rc<RefCell<Scene>>,
        target: NodeId,
        event: InputEvent,
        modifiers: Modifiers,
        click_count: u8,
        alone: bool,
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
                | InputEvent::Pinch { .. }
                | InputEvent::PointerEnter
                | InputEvent::PointerLeave
                | InputEvent::PointerCancel
                | InputEvent::Activate
                // A drop says what keys are held (Option copies a dragged
                // selection rather than moving it).
                | InputEvent::Drag(_)
        ) {
            modifiers
        } else {
            Modifiers::default()
        };
        let mut path = vec![target];
        if !alone {
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
            drag_threshold: self.drag_threshold(),
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
    /// Make `root` a focus group (ARIA's composite widgets: a list box, a
    /// toolbar, tabs), or with `None` stop: it is one Tab stop, the member
    /// last focused in it (else the one `set_group_current` named, else
    /// its first), and `axis`'s arrow keys, Home and End move focus among
    /// its members, when what has focus does not use them itself.
    pub fn set_focus_group(&self, root: NodeId, axis: Option<FocusGroup>) {
        let mut s = self.state.borrow_mut();
        match axis {
            Some(axis) => {
                let (current, chooses) = s
                    .groups
                    .get(&root)
                    .map_or((None, false), |g| (g.current, g.chooses));
                s.groups.insert(
                    root,
                    Group {
                        axis: Some(axis),
                        current,
                        chooses,
                    },
                );
            }
            None => {
                s.groups.remove(&root);
            }
        }
    }
    /// Make `root` one Tab stop, as `set_focus_group` does, leaving its
    /// arrow keys to what owns it (a virtual list moving through rows it
    /// mounts as it goes).
    pub fn set_tab_stop_group(&self, root: NodeId) {
        let mut s = self.state.borrow_mut();
        let current = s.groups.get(&root).and_then(|g| g.current);
        s.groups.insert(
            root,
            Group {
                axis: None,
                current,
                chooses: false,
            },
        );
    }
    /// Make focus group `root`'s arrows choose what they move to (activate
    /// it), wrapping from one end to the other, as a radio group's do in a
    /// browser and in AppKit.
    pub(crate) fn set_group_chooses(&self, root: NodeId, chooses: bool) {
        if let Some(group) = self.state.borrow_mut().groups.get_mut(&root) {
            group.chooses = chooses;
        }
    }
    /// Make `member` the one its focus group's Tab stop lands on: the
    /// selected option, the active tab.
    pub fn set_group_current(&self, scene: &Scene, member: NodeId) {
        if let Some(group) = self.group_of(scene, member) {
            let mut s = self.state.borrow_mut();
            if let Some(g) = s.groups.get_mut(&group) {
                g.current = Some(member);
            }
        }
    }
    /// The innermost focus group holding `id` (not `id` itself).
    fn group_of(&self, scene: &Scene, id: NodeId) -> Option<NodeId> {
        let s = self.state.borrow();
        if s.groups.is_empty() || !scene.contains(id) {
            return None;
        }
        scene.ancestors(id).find(|a| s.groups.contains_key(a))
    }
    /// The focusable, enabled nodes under `root` in document order, with
    /// their tab index.
    fn focusables(&self, scene: &Scene, root: NodeId) -> Vec<(NodeId, i32)> {
        let mut stack = vec![root];
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
        ordered
    }
    /// The arrow, Home or End `key` moving focus within the focused node's
    /// group: the member to focus, if the key is the group's.
    fn group_step(&self, scene: &Scene, key: &Key) -> Option<NodeId> {
        let focused = self.focused()?;
        let group = self.group_of(scene, focused)?;
        let (axis, wraps) = {
            let s = self.state.borrow();
            let g = s.groups.get(&group)?;
            (g.axis?, g.chooses)
        };
        enum Step {
            Back,
            On,
            First,
            Last,
        }
        let vertical = matches!(axis, FocusGroup::Vertical | FocusGroup::Both);
        let horizontal = matches!(axis, FocusGroup::Horizontal | FocusGroup::Both);
        // Right to left, the next member is to the left.
        let rtl = scene.direction(group) == crate::scene::Direction::Rtl;
        let (left, right) = if rtl {
            (Step::On, Step::Back)
        } else {
            (Step::Back, Step::On)
        };
        let step = match key {
            Key::ArrowUp if vertical => Step::Back,
            Key::ArrowDown if vertical => Step::On,
            Key::ArrowLeft if horizontal => left,
            Key::ArrowRight if horizontal => right,
            Key::Home => Step::First,
            Key::End => Step::Last,
            _ => return None,
        };
        // Its own members: not those of a group inside it.
        let members: Vec<_> = self
            .focusables(scene, group)
            .into_iter()
            .map(|(id, _)| id)
            .filter(|id| *id != group && self.group_of(scene, *id) == Some(group))
            .collect();
        let at = members.iter().position(|id| *id == focused)?;
        match step {
            Step::Back if wraps && at == 0 => members.last().copied(),
            Step::On if wraps && at + 1 == members.len() => members.first().copied(),
            Step::Back => at.checked_sub(1).map(|i| members[i]),
            Step::On => members.get(at + 1).copied(),
            Step::First => members.first().copied(),
            Step::Last => members.last().copied(),
        }
        .filter(|id| *id != focused)
    }
    /// Where focus goes when `gone`, which holds what has focus, leaves
    /// and no focusable ancestor stays: the next Tab stop after it, else
    /// the one before, as a native toolkit moves focus on rather than to
    /// the window. In a focus group, the group's own stop.
    pub fn successor(&self, scene: &Scene, gone: NodeId) -> Option<NodeId> {
        let ordered = self.focusables(scene, scene.root());
        let inside = |id: NodeId| scene.is_within(id, gone);
        let at = ordered.iter().position(|(id, _)| inside(*id))?;
        let pick = ordered[at..]
            .iter()
            .find(|(id, _)| !inside(*id))
            .or_else(|| ordered[..at].iter().rev().find(|(id, _)| !inside(*id)))?
            .0;
        let current = self
            .group_of(scene, pick)
            .and_then(|group| {
                self.state
                    .borrow()
                    .groups
                    .get(&group)
                    .and_then(|g| g.current)
            })
            .filter(|member| {
                scene.contains(*member)
                    && !inside(*member)
                    && self.options(*member).is_some_and(|o| o.focusable)
                    && self.enabled(scene, *member)
            });
        Some(current.unwrap_or(pick))
    }
    fn next_focus(&self, scene: &Scene, reverse: bool) -> Option<NodeId> {
        // Within a modal scope Tab cycles its controls alone: past its last
        // it comes round to its first, rather than leaving it.
        let from = self
            .state
            .borrow()
            .scopes
            .last()
            .filter(|scope| !scope.porous && scene.contains(scope.root))
            .map_or(scene.root(), |scope| scope.root);
        let mut ordered = self.focusables(scene, from);
        // A focus group is one stop: the member that has focus, else its
        // current, else its first.
        if !self.state.borrow().groups.is_empty() {
            let focused = self.focused();
            let mut seen: Vec<NodeId> = Vec::new();
            let mut stops = Vec::with_capacity(ordered.len());
            for (id, index) in ordered {
                // A focusable group root (a list taking keys before any row
                // has focus) is its group's too: the stop only when no member
                // can take it.
                let own = self.state.borrow().groups.contains_key(&id);
                let Some(group) = own.then_some(id).or_else(|| self.group_of(scene, id)) else {
                    stops.push((id, index));
                    continue;
                };
                if seen.contains(&group) {
                    continue;
                }
                seen.push(group);
                let members: Vec<_> = self
                    .focusables(scene, group)
                    .into_iter()
                    .filter(|(m, _)| self.group_of(scene, *m) == Some(group))
                    .collect();
                let current = self
                    .state
                    .borrow()
                    .groups
                    .get(&group)
                    .and_then(|g| g.current);
                let stop = members
                    .iter()
                    .find(|(m, _)| Some(*m) == focused)
                    .or_else(|| members.iter().find(|(m, _)| Some(*m) == current))
                    .copied()
                    .unwrap_or((id, index));
                stops.push(stop);
            }
            ordered = stops;
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
    /// Route `event` to `target`: capture down from the root, the target,
    /// then bubbling up.
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
                let (options, hollow) = self.options_hollow(id);
                if options.is_some_and(|options| options.pass_through)
                    || (hollow && !scene.displays_content(id))
                {
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
    /// Route `event` as the platform delivers it: pointer events to what is
    /// under the pointer or captures it, keys to the focused node.
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
        // A handler that panics skips this dispatch's own clean-up (a
        // released press, a capture ending): if it unwinds, the gesture it
        // was part of ends here, so the next press reaches what it is on
        // rather than the node the failed one captured.
        struct Unwinding<'a>(&'a RefCell<State>);
        impl Drop for Unwinding<'_> {
            fn drop(&mut self) {
                if std::thread::panicking()
                    && let Ok(mut state) = self.0.try_borrow_mut()
                {
                    state.captured = None;
                    state.capture_button = None;
                    state.pressed = None;
                    state.key_pressed = None;
                }
            }
        }
        let _unwinding = Unwinding(&self.state);
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
                // The physical key, for bindings of one (`KeyBinding::code`).
                let code = self.state.borrow().key_code.clone();
                loop {
                    let path = self.action_path(&scene.borrow());
                    let resolution = self.actions.borrow_mut().resolve(
                        path,
                        crate::actions::Keystroke::new(key.clone(), *modifiers),
                        code.as_deref(),
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
        let mut capture = self
            .captured()
            .filter(|id| self.enabled(&scene.borrow(), *id));
        if capture.is_none() {
            self.set_capture(None, &event);
        }
        // The rest of a wheel gesture goes to the scroll view it latched
        // to, alone, as macOS latches one: it neither chains to the views
        // around it at an edge nor moves to one that slides under the
        // pointer. A pause ends the gesture.
        if let InputEvent::Scroll {
            delta_x, delta_y, ..
        } = &event
        {
            let now = Instant::now();
            let horizontal = delta_x.abs() > delta_y.abs();
            let latched = self.state.borrow().scroll_latch.filter(|(node, axis, at)| {
                *axis == horizontal
                    && now.saturating_duration_since(*at) < SCROLL_LATCH
                    && scene.borrow().contains(*node)
                    && self.enabled(&scene.borrow(), *node)
            });
            if let Some((node, ..)) = latched {
                let ctx =
                    self.route_pointer(scene, node, event, modifiers.unwrap_or_default(), 0, true);
                self.state.borrow_mut().scroll_latch = Some((node, horizontal, now));
                return DispatchResult {
                    target: Some(node),
                    default_prevented: ctx.prevented,
                    focus_changed: false,
                };
            }
        }
        let mut hit = match &event {
            InputEvent::PointerDown { x, y, .. }
            | InputEvent::PointerUp { x, y, .. }
            | InputEvent::PointerMove { x, y }
            | InputEvent::Scroll { x, y, .. }
            | InputEvent::Pinch { x, y, .. }
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
        // Listeners told of hover may have removed what the move goes to:
        // the node under it, or the one capturing it (whose capture ends).
        if matches!(
            event,
            InputEvent::PointerMove { .. } | InputEvent::PointerLeave
        ) && self.set_hover(scene, hit)
        {
            let scene = scene.borrow();
            hit = hit.filter(|id| scene.contains(*id));
            if capture.is_some_and(|id| !scene.contains(id)) {
                capture = None;
                self.set_capture(None, &event);
            }
        }
        let mut target = match &event {
            InputEvent::PointerDown { .. }
            | InputEvent::PointerUp { .. }
            | InputEvent::PointerMove { .. } => capture.or(hit),
            InputEvent::Scroll { .. }
            | InputEvent::Pinch { .. }
            | InputEvent::FileHover { .. }
            | InputEvent::FileDrop { .. }
            | InputEvent::FilesDrop { .. }
            | InputEvent::FilesDropRejected { .. } => hit,
            InputEvent::FileHoverCancelled => self.state.borrow_mut().file_hovered.take(),
            InputEvent::PointerCancel => capture,
            InputEvent::PointerLeave => None,
            // Empty or disabled modal contents still need dismissal keys.
            // With nothing focused, a view that takes unfocused keys does.
            InputEvent::KeyDown { .. } | InputEvent::KeyUp { .. } => {
                self.focused().or_else(|| self.focus_scope()).or_else(|| {
                    let scene = scene.borrow();
                    let mut state = self.state.borrow_mut();
                    state.unfocused_keys.retain(|n| scene.contains(*n));
                    state.unfocused_keys.last().copied()
                })
            }
            _ => self.focused(),
        };
        if matches!(event, InputEvent::FileHover { .. }) {
            let old = self.state.borrow_mut().file_hovered.take();
            if old != target
                && let Some(old) = old.filter(|id| scene.borrow().contains(*id))
            {
                self.dispatch_to(scene, old, InputEvent::FileHoverCancelled);
                // Its listeners may have removed where the files are now.
                target = target.filter(|id| scene.borrow().contains(*id));
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
        // This press's, whatever becomes of it (`keep_focus_for_press`).
        let keep_focus = matches!(event, InputEvent::PointerDown { .. })
            && std::mem::take(&mut self.state.borrow_mut().keep_focus);
        let mut prevented = false;
        if let Some(id) = target {
            let ctx = self.route_pointer(
                scene,
                id,
                event.clone(),
                modifiers.unwrap_or_default(),
                click_count,
                false,
            );
            prevented = ctx.prevented;
            if let InputEvent::Scroll {
                delta_x, delta_y, ..
            } = event
            {
                let horizontal = delta_x.abs() > delta_y.abs();
                self.state.borrow_mut().scroll_latch = ctx
                    .prevented_by
                    .filter(|_| ctx.prevented)
                    .map(|node| (node, horizontal, Instant::now()));
            }
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
                    if !keep_focus && (target.is_some() || self.focus_scope().is_none()) {
                        self.focus(scene, focus);
                    }
                }
                // Tab and Shift-Tab, and Control with them (AppKit's and
                // GTK's way out of a text view that takes Tab); Option- and
                // Command-Tab are the system's, or a field's literal tab.
                InputEvent::KeyDown {
                    key: Key::Tab,
                    modifiers,
                    ..
                } if !modifiers.alt && !modifiers.meta => {
                    let next = self.next_focus(&scene.borrow(), modifiers.shift);
                    self.focus(scene, next);
                    prevented = true;
                }
                InputEvent::KeyDown {
                    key:
                        key @ (Key::ArrowUp
                        | Key::ArrowDown
                        | Key::ArrowLeft
                        | Key::ArrowRight
                        | Key::Home
                        | Key::End),
                    modifiers,
                    ..
                } if *modifiers == Modifiers::default() => {
                    let next = self.group_step(&scene.borrow(), key);
                    if next.is_some() {
                        self.focus(scene, next);
                        prevented = true;
                        // A radio group's arrows choose too.
                        let chooses = next.and_then(|id| {
                            let group = self.group_of(&scene.borrow(), id)?;
                            let chooses = self.state.borrow().groups.get(&group)?.chooses;
                            chooses.then_some(id)
                        });
                        if let Some(id) = chooses.filter(|id| self.focused() == Some(*id)) {
                            self.dispatch_to(scene, id, InputEvent::Activate);
                        }
                    }
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
                // Enter activates as it goes down, as a browser's button and
                // AppKit's default button take it.
                InputEvent::KeyDown {
                    key: Key::Enter,
                    repeat: false,
                    ..
                } => {
                    self.state.borrow_mut().key_pressed = None;
                    if let Some(id) = self
                        .still_targeted(scene, target)
                        .filter(|id| self.activates(*id, &Key::Enter))
                    {
                        self.dispatch_to(scene, id, InputEvent::Activate);
                    }
                }
                // Space as it comes up: pressed, it shows pressed until then.
                InputEvent::KeyDown {
                    key: Key::Space,
                    repeat: false,
                    ..
                } => {
                    let pressed = self
                        .still_targeted(scene, target)
                        .filter(|id| self.activates(*id, &Key::Space))
                        .map(|id| (id, Key::Space));
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
    fn click_tracker_time_distance_button_target_and_count_are_deterministic() {
        let (_, a, b) = setup();
        let mut tracker = ClickTracker::default();
        let now = Instant::now();
        // Past a triple's, on counting (the web's `detail`), not round again.
        for (index, expected) in [1, 2, 3, 4].into_iter().enumerate() {
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
            5
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
    fn a_longer_double_click_time_counts_a_slower_second_press() {
        let (_, a, _) = setup();
        let input = InputDispatcher::new();
        let now = Instant::now();
        let press = |at: u64| {
            input.state.borrow_mut().clicks.press(
                a,
                PointerButton::Primary,
                (0., 0.),
                now + Duration::from_millis(at),
            )
        };
        assert_eq!(press(0), 1);
        assert_eq!(press(700), 1, "past the default 500 ms");
        input.set_double_click_time(Duration::from_millis(900));
        assert_eq!(press(1400), 2);
        input.set_double_click_time(Duration::ZERO);
        assert_eq!(input.state.borrow().clicks.1, Duration::from_millis(100));
    }
    #[test]
    fn a_second_press_counts_inside_the_double_click_box_alone() {
        let (_, a, _) = setup();
        let input = InputDispatcher::new();
        let now = Instant::now();
        let press = |at: u64, x: f32, y: f32| {
            input.state.borrow_mut().clicks.press(
                a,
                PointerButton::Primary,
                (x, y),
                now + Duration::from_millis(at),
            )
        };
        // Until the host gives a box: within 4 points in any direction.
        assert_eq!(press(0, 0., 0.), 1);
        assert_eq!(press(100, 3., 0.), 2);
        // Windows' default box, 4 by 4 pixels: 2 either side.
        input.set_double_click_area(4., 4.);
        assert_eq!(press(1000, 0., 0.), 1);
        assert_eq!(press(1100, 3., 0.), 1, "outside the box");
        assert_eq!(press(2000, 0., 0.), 1);
        assert_eq!(press(2100, 2., -2.), 2, "on its edge");
        // Widened, as a user whose hands shake sets it: 20 by 10.
        input.set_double_click_area(20., 10.);
        assert_eq!(press(3000, 0., 0.), 1);
        assert_eq!(press(3100, 9., 4.), 2);
        assert_eq!(
            press(3200, 9., 10.),
            1,
            "below the last press's box: a new first"
        );
        // Moving out of the box ends the chance of a double click.
        input.state.borrow_mut().clicks.moved(9., 16.);
        assert!(input.state.borrow().clicks.0.is_none());
        input.set_double_click_area(f32::NAN, 4.);
        assert_eq!(input.state.borrow().clicks.2, Some((10., 5.)));
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
    fn tab_moves_focus_with_shift_or_control_but_not_option_or_command() {
        let (scene, a, b) = setup();
        let d = InputDispatcher::new();
        let opts = NodeInput {
            focusable: true,
            ..Default::default()
        };
        let _a = d.register(a, opts, |_| {});
        let _b = d.register(b, opts, |_| {});
        let tab = |modifiers| InputEvent::KeyDown {
            key: Key::Tab,
            modifiers,
            repeat: false,
        };
        d.focus(&scene, Some(a));
        let alt = Modifiers {
            alt: true,
            ..Default::default()
        };
        let meta = Modifiers {
            meta: true,
            ..Default::default()
        };
        for held in [alt, meta] {
            let result = d.dispatch(&scene, tab(held));
            assert_eq!(d.focused(), Some(a), "{held:?}");
            assert!(!result.default_prevented, "{held:?} goes on unhandled");
        }
        // Control-Tab: AppKit's and GTK's way out of a text view.
        let control = Modifiers {
            control: true,
            ..Default::default()
        };
        d.dispatch(&scene, tab(control));
        assert_eq!(d.focused(), Some(b));
        let back = Modifiers {
            shift: true,
            control: true,
            ..Default::default()
        };
        d.dispatch(&scene, tab(back));
        assert_eq!(d.focused(), Some(a));
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
        // Enter activates as it goes down, once, and drops a pending Space.
        input.dispatch(&scene, down(Key::Space, false));
        input.dispatch(&scene, down(Key::Enter, false));
        assert_eq!(clicks.get(), 2);
        input.dispatch(&scene, down(Key::Enter, true));
        input.dispatch(&scene, down(Key::Space, true));
        input.dispatch(&scene, up(Key::Space));
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
pub use drag::{DragEnded, DragEvent, DragItem, DragPhase, DragState, DraggedText, SPRING_DELAY};
