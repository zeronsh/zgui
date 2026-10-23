//! Retained widgets with reactive values, keyboard input and accessibility semantics.
use crate::{
    input::{EventPhase, InputBinding, InputDispatcher, InputEvent, Key, PointerButton},
    reactive::{Effect, Runtime, Signal},
    scene::{Color, Insets, Layout, NodeId, NodeKind, Rect, Scene, Style, Transform},
    semantics::{Role, SemanticNode, Semantics},
    text_edit::TextEditor,
    text_layout::{FontStyle, TextAffinity, TextLayout, TextPosition},
    theme::{ColorToken, ThemeScope, Themes, tok},
};
use std::{
    any::Any,
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::{Rc, Weak},
    time::{Duration, Instant},
};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Default)]
struct Owned {
    effects: Vec<Effect>,
    bindings: Vec<InputBinding>,
    resources: Vec<Box<dyn Any>>,
}
/// An editor's box size, style, shaper revision and content insets.
type EditorGeometry = (Rect, Style, u64, Insets);
#[derive(Clone)]
pub struct EditorHandle {
    multiline: bool,
    read_only: Rc<Cell<bool>>,
    /// Retained caret geometry for native IME candidate-window placement.
    pub caret: NodeId,
    pub node: NodeId,
    pub editor: Rc<RefCell<TextEditor>>,
    pub value: Signal<String>,
    refresh: Rc<dyn Fn()>,
    typography: Rc<RefCell<(Color, f32, FontStyle)>>,
    wrap: Rc<Cell<bool>>,
    line_numbers: Rc<Cell<bool>>,
    preferred_x: Rc<Cell<Option<f32>>>,
    text: NodeId,
    scene: Rc<RefCell<Scene>>,
    /// What the last refresh laid the text out for: the box's size, its
    /// style, the shaper revision and its content insets (which a border
    /// in its kind changes too).
    geometry: Rc<RefCell<Option<EditorGeometry>>>,
    position: Rc<Cell<TextPosition>>,
    hint: Rc<EditorHint>,
}
/// What an editor shows while it is empty, and whether focus selects all.
#[derive(Default)]
struct EditorHint {
    /// The placeholder and its colour (`None`: the theme's `muted`).
    placeholder: RefCell<Option<(std::sync::Arc<str>, Option<Color>)>>,
    select_on_focus: Cell<bool>,
    /// A password field: its text shows masked and never leaves it.
    secure: Cell<bool>,
}
impl EditorHandle {
    /// The selection as UTF-8 byte offsets into the committed text,
    /// `(anchor, focus)`: equal for a caret, `focus` where the caret is.
    pub fn selection(&self) -> (usize, usize) {
        let selection = self.editor.borrow().selection();
        (selection.anchor, selection.focus)
    }
    /// Select from `anchor` to `focus` (UTF-8 byte offsets, clamped to the
    /// text and moved back to a grapheme boundary), as a drag would.
    pub fn set_selection(&self, anchor: usize, focus: usize) {
        let focus = {
            let mut editor = self.editor.borrow_mut();
            editor.set_selection(anchor, focus);
            editor.selection().focus
        };
        self.position.set(TextPosition {
            byte_offset: focus,
            affinity: TextAffinity::After,
        });
        self.preferred_x.set(None);
        self.refresh();
    }
    /// Select the whole text.
    pub fn select_all(&self) {
        let end = self.editor.borrow().text().len();
        self.set_selection(0, end);
    }
    /// Select all whenever the editor takes keyboard focus (a click, Tab,
    /// a program), as a browser's address bar does: the first click selects
    /// everything, a second places the caret. A window coming back to the
    /// front keeps the selection it had.
    pub fn set_select_on_focus(&self, select: bool) {
        self.hint.select_on_focus.set(select);
    }
    /// A password field: its text shows as a bullet per character (each in
    /// that character's place, so carets and selections work), and copying
    /// or cutting it gives nothing.
    pub fn set_secure(&self, secure: bool) {
        self.hint.secure.set(secure);
    }
    /// Text shown, never edited, while the editor is empty: in its own
    /// typography, coloured `color` (`None`: the theme's `muted`). It is
    /// not part of the value and does not change the editor's size;
    /// assistive technology reads it as the placeholder.
    pub fn set_placeholder(&self, text: Option<&str>, color: Option<Color>) {
        let next = text
            .filter(|text| !text.is_empty())
            .map(|text| (std::sync::Arc::<str>::from(text), color));
        let mut current = self.hint.placeholder.borrow_mut();
        if *current == next {
            return;
        }
        *current = next;
        drop(current);
        self.refresh();
    }
    /// The placeholder, if any.
    pub fn placeholder(&self) -> Option<String> {
        self.hint
            .placeholder
            .borrow()
            .as_ref()
            .map(|(text, _)| text.to_string())
    }
    /// Whether user editing is blocked. Selection, copying and model writes remain available.
    pub fn is_read_only(&self) -> bool {
        self.read_only.get()
    }
    /// Change editing policy without moving focus or changing committed text/history.
    /// Entering read-only mode cancels an existing composition.
    pub fn set_read_only(&self, read_only: bool) {
        if self.read_only.replace(read_only) == read_only {
            return;
        }
        if read_only {
            self.editor.borrow_mut().cancel_preedit();
        }
        self.refresh();
    }

    /// Enable soft wrapping for multiline controls. Single-line controls stay unwrapped.
    pub fn set_wrap(&self, wrap: bool) {
        let wrap = wrap && self.multiline;
        if self.wrap.replace(wrap) != wrap {
            self.preferred_x.set(None);
            self.refresh();
        }
    }
    /// Number a multiline editor's lines in a gutter at its left (a code
    /// editor's): each logical line's number beside its first row, the
    /// caret's brighter, only those in view built. Single-line controls
    /// have none.
    pub fn set_line_numbers(&self, numbered: bool) {
        let numbered = numbered && self.multiline;
        if self.line_numbers.replace(numbered) != numbered {
            self.refresh();
        }
    }
    /// Update rendering and editing metrics together, including selection and IME geometry.
    pub fn set_typography(&self, color: Color, size: f32, font: FontStyle) {
        self.set_text_style(color, size, font, self.wrap.get());
    }
    pub(crate) fn set_text_style(&self, color: Color, size: f32, mut font: FontStyle, wrap: bool) {
        let wrap = wrap && self.multiline;
        let wrap_changed = self.wrap.replace(wrap) != wrap;
        font.weight = font.weight.clamp(1, 1000);
        let next = (color, size.max(1.), font);
        let previous = self.typography.borrow().clone();
        if previous == next && !wrap_changed {
            return;
        }
        let metrics_changed = wrap_changed || previous.1 != next.1 || previous.2 != next.2;
        *self.typography.borrow_mut() = next;
        // A placeholder showing keeps its own colour: draw it again.
        if metrics_changed || self.hint.placeholder.borrow().is_some() {
            self.preferred_x.set(None);
            self.refresh();
        } else {
            let mut scene = self.scene.borrow_mut();
            if !scene.contains(self.text) {
                return;
            }
            if let NodeKind::Text {
                text, font_size, ..
            } = scene.kind(self.text).clone()
            {
                scene.set_kind(
                    self.text,
                    NodeKind::Text {
                        text,
                        font_size,
                        color,
                    },
                );
            }
            scene.set_kind(self.caret, NodeKind::Rect(color));
        }
    }
    fn refresh_if_resized(&self) {
        let scene = self.scene.borrow();
        if !scene.contains(self.node) {
            return;
        }
        let bounds = scene.bounds(self.node);
        let next = (
            Rect::new(0., 0., bounds.width, bounds.height),
            scene.style(self.node),
            scene.text_shaper_revision(),
            scene.content_insets(self.node),
        );
        let changed = self.geometry.borrow().as_ref() != Some(&next);
        drop(scene);
        if changed {
            self.preferred_x.set(None);
            self.refresh();
        }
    }

    pub fn refresh(&self) {
        (self.refresh)();
    }
    pub fn copy(&self) -> String {
        // A password field's text never leaves it.
        if self.hint.secure.get() {
            return String::new();
        }
        self.editor.borrow().selected_text().to_owned()
    }
    /// Copy the selection and delete it when editable. Read-only cut is copy-only.
    pub fn cut(&self) -> String {
        if self.is_read_only() {
            return self.copy();
        }
        self.preferred_x.set(None);
        let text = self.copy();
        if !text.is_empty() {
            self.editor.borrow_mut().insert("");
            let next = self.editor.borrow().text().to_owned();
            self.value.set(next);
            self.refresh();
        }
        text
    }
    pub fn paste(&self, text: &str) {
        if self.is_read_only() {
            return;
        }
        self.preferred_x.set(None);
        self.editor
            .borrow_mut()
            .insert(&normalize_text(text, self.multiline));
        let next = self.editor.borrow().text().to_owned();
        self.value.set(next);
        self.refresh();
    }
}
/// Layout notifications repeatedly changed their own allocation without settling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayoutFeedbackError {
    pub passes: usize,
}
impl std::fmt::Display for LayoutFeedbackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "layout allocation did not settle after {} passes; check size-dependent view mutations",
            self.passes
        )
    }
}
impl std::error::Error for LayoutFeedbackError {}
struct FramePreparationGuard<'a>(&'a Cell<bool>);
impl Drop for FramePreparationGuard<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

/// One retained document. Application code keeps returned node IDs as stable widget handles.
pub struct Ui {
    pub runtime: Runtime,
    pub scene: Rc<RefCell<Scene>>,
    pub input: InputDispatcher,
    pub semantics: Rc<RefCell<Semantics>>,
    storage: Rc<UiStorage>,
}
// One focused-editor snapshot per document. Large or unaccounted custom layouts
// remain usable but are never retained by this reuse entry.
const EDITOR_CACHE_TEXT_LIMIT: usize = 64 * 1024;
const EDITOR_CACHE_WEIGHT_LIMIT: usize = 1024 * 1024;
struct EditorLayoutEntry {
    node: NodeId,
    text: String,
    size: u32,
    width: Option<u32>,
    font: FontStyle,
    revision: u64,
    layout: Rc<dyn TextLayout>,
}

/// A numbered editor's gutter: space before the numbers, and between them
/// and the text.
const GUTTER_INSET: f32 = 4.;
const GUTTER_GAP: f32 = 14.;
/// How many times `byte` occurs in `bytes`.
fn memchr_count(byte: u8, bytes: &[u8]) -> usize {
    bytes.iter().filter(|&&b| b == byte).count()
}

fn editor_layout(
    storage: &Weak<UiStorage>,
    scene: &Scene,
    node: NodeId,
    text: &str,
    size: f32,
    width: Option<f32>,
    font: &FontStyle,
) -> Rc<dyn TextLayout> {
    let storage = storage.upgrade();
    let eligible = storage
        .as_ref()
        .is_some_and(|s| s.input.focused() == Some(node))
        && text.len() <= EDITOR_CACHE_TEXT_LIMIT;
    let revision = scene.text_shaper_revision();
    let width_bits = width.map(f32::to_bits);
    if eligible {
        let cache = storage.as_ref().unwrap().editor_layout.borrow();
        if let Some(entry) = cache.as_ref()
            && entry.node == node
            && entry.size == size.to_bits()
            && entry.width == width_bits
            && entry.font == *font
            && entry.revision == revision
            && entry.text == text
        {
            return entry.layout.clone();
        }
    }
    // Drop stale focused snapshots even when their replacement is too large.
    if let Some(storage) = &storage {
        let old = {
            let mut cache = storage.editor_layout.borrow_mut();
            if storage.input.focused() == Some(node)
                || cache.as_ref().is_some_and(|entry| entry.node == node)
            {
                cache.take()
            } else {
                None
            }
        };
        drop(old);
    }
    let layout: Rc<dyn TextLayout> = scene.shape_text_with_font(text, size, width, font).into();
    if eligible {
        let key_weight = std::mem::size_of::<EditorLayoutEntry>()
            .saturating_add(text.len())
            .saturating_add(font.storage_bytes())
            .saturating_add(2 * std::mem::size_of::<usize>());
        if layout
            .cache_weight()
            .is_some_and(|weight| weight.saturating_add(key_weight) <= EDITOR_CACHE_WEIGHT_LIMIT)
            && storage.as_ref().unwrap().input.focused() == Some(node)
        {
            let old = storage
                .as_ref()
                .unwrap()
                .editor_layout
                .borrow_mut()
                .replace(EditorLayoutEntry {
                    node,
                    text: text.to_owned(),
                    size: size.to_bits(),
                    width: width_bits,
                    font: font.clone(),
                    revision,
                    layout: layout.clone(),
                });
            drop(old);
        }
    }
    layout
}

type InteractionRefresh = Rc<RefCell<Option<Weak<dyn Fn()>>>>;

struct EditorInteraction {
    owner: NodeId,
    generation: u64,
    gesture: Rc<EditorDrag>,
    can_scroll: Rc<dyn Fn() -> bool>,
    tick: Rc<dyn Fn(f64)>,
}
struct InteractionDeadline {
    job: Rc<EditorInteraction>,
    last: Instant,
    next: Instant,
}
#[derive(Default)]
struct EditorDrag {
    mode: Cell<Option<EditorPointerSelection>>,
    pointer: Cell<(f32, f32)>,
    generation: Cell<u64>,
}
impl EditorDrag {
    fn cancel(&self) {
        self.mode.set(None);
        self.generation.set(self.generation.get().wrapping_add(1));
    }
}
struct UiStorage {
    runtime: Runtime,
    themes: Rc<Themes>,
    /// The theme scope of the element being mounted (`Ui::enter_theme`).
    mount_theme: RefCell<Option<Rc<ThemeScope>>>,
    text_geometry_revision: Signal<u64>,
    presented: Signal<bool>,
    scene: Rc<RefCell<Scene>>,
    input: InputDispatcher,
    semantics: Rc<RefCell<Semantics>>,
    owned: RefCell<HashMap<NodeId, Owned>>,
    editors: RefCell<HashMap<NodeId, EditorHandle>>,
    editor_layout: RefCell<Option<EditorLayoutEntry>>,
    editor_layout_revision: Cell<Option<u64>>,
    bounds_geometry_revision: Cell<Option<u64>>,
    preparing_frame: Cell<bool>,
    /// Where focus falls once the frame settles, if nothing took it: the
    /// nearest focusable ancestor of a removed focused node (`Ui::remove`).
    refocus: Cell<Option<NodeId>>,
    content_sizes: RefCell<HashMap<NodeId, Signal<(f32, f32)>>>,
    /// Nodes' laid-out sizes (`observe_size`): read only when layout runs.
    observed_sizes: RefCell<HashMap<NodeId, Signal<(f32, f32)>>>,
    observed_bounds: RefCell<HashMap<NodeId, Signal<Rect>>>,
    observed_visibility: RefCell<HashMap<NodeId, Signal<bool>>>,
    /// Run once after the next frame preparation's layout.
    after_layout: RefCell<Vec<Box<dyn FnOnce()>>>,
    /// Run after every layout pass that changes the layout, by owning node.
    layout_listeners: RefCell<HashMap<NodeId, Vec<LayoutListener>>>,
    mount_initialization: RefCell<Option<Rc<MountInitialization>>>,
    interaction: RefCell<Option<InteractionDeadline>>,
    advancing_interaction: Cell<bool>,
    window_gesture: Cell<Option<WindowGesture>>,
    /// Display text's selection (`View::selectable`).
    selection: RefCell<crate::selection::State>,
    /// What the last primary press went to: the keyboard scrolls the scroll
    /// view around it (`scroll_keys`).
    last_pressed: Cell<Option<NodeId>>,
}
/// A press in a window drag region (`View::window_drag`), for the host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowGesture {
    /// Move the window with the pointer: the press is still down.
    Drag,
    /// The second press of a double click: zoom or minimize the window, as
    /// the platform's setting for its title bar says.
    DoubleClick,
}
type LayoutListener = Rc<RefCell<Box<dyn FnMut()>>>;
/// A handle to a `Ui` that does not keep it alive (`Ui::downgrade`).
#[derive(Clone)]
pub struct WeakUi {
    storage: Weak<UiStorage>,
}
impl WeakUi {
    pub fn upgrade(&self) -> Option<Ui> {
        self.storage.upgrade().map(Ui::from_storage)
    }
}
/// Restores the previous mount theme scope (`Ui::enter_theme`), unless
/// entering changed nothing (the usual case: a child of an element mounts
/// in its parent's scope).
pub(crate) struct ThemeGuard {
    restore: Option<(Weak<UiStorage>, Option<Rc<ThemeScope>>)>,
}
impl Drop for ThemeGuard {
    fn drop(&mut self) {
        if let Some((storage, previous)) = self.restore.take()
            && let Some(storage) = storage.upgrade()
        {
            let restored = storage.mount_theme.replace(previous);
            drop(restored);
        }
    }
}
/// Ownership cleanup for the initial bindings of one public component mount.
/// It does not roll back model writes or catch later reactive update failures.
pub(crate) struct MountInitialization {
    ui: WeakUi,
    root: Cell<Option<NodeId>>,
    failed: Cell<bool>,
}
impl MountInitialization {
    pub(crate) fn set_root(&self, root: NodeId) {
        self.root.set(Some(root));
    }
    pub(crate) fn enter(self: &Rc<Self>) -> MountInitializationScope {
        let previous = self
            .ui
            .upgrade()
            .and_then(|ui| ui.storage.mount_initialization.replace(Some(self.clone())));
        MountInitializationScope {
            token: self.clone(),
            previous,
        }
    }
    fn fail(&self) {
        if self.failed.replace(true) {
            return;
        }
        if let Some(mut ui) = self.ui.upgrade()
            && let Some(root) = self.root.get()
        {
            ui.remove(root);
        }
    }
}
pub(crate) struct MountInitializationScope {
    token: Rc<MountInitialization>,
    previous: Option<Rc<MountInitialization>>,
}
impl Drop for MountInitializationScope {
    fn drop(&mut self) {
        if let Some(ui) = self.token.ui.upgrade() {
            let old = ui
                .storage
                .mount_initialization
                .replace(self.previous.take());
            drop(old);
        }
    }
}
struct InitialBindingGuard {
    token: Rc<MountInitialization>,
    scope: Option<MountInitializationScope>,
    completed: bool,
}
impl Drop for InitialBindingGuard {
    fn drop(&mut self) {
        // User resource destructors can mount new siblings during cleanup.
        // They must not inherit the failed initialization scope.
        drop(self.scope.take());
        if !self.completed && std::thread::panicking() {
            self.token.fail();
        }
    }
}

impl Drop for UiStorage {
    fn drop(&mut self) {
        let owned = std::mem::take(self.owned.get_mut());
        let editors = std::mem::take(self.editors.get_mut());
        for id in owned.keys() {
            self.input.unregister(*id);
        }
        drop(owned);
        drop(editors);
        self.input.validate_drag(&self.scene);
    }
}
impl Ui {
    pub fn new(width: f32, height: f32) -> Self {
        let runtime = Runtime::new();
        let storage = Rc::new(UiStorage {
            themes: Themes::of(&runtime),
            mount_theme: RefCell::new(None),
            text_geometry_revision: runtime.signal(0),
            presented: runtime.signal(true),
            runtime,
            scene: Rc::new(RefCell::new(Scene::new(width, height))),
            input: InputDispatcher::new(),
            semantics: Rc::new(RefCell::new(Semantics::new())),
            owned: RefCell::new(HashMap::new()),
            editors: RefCell::new(HashMap::new()),
            editor_layout: RefCell::new(None),
            editor_layout_revision: Cell::new(None),
            bounds_geometry_revision: Cell::new(None),
            preparing_frame: Cell::new(false),
            refocus: Cell::new(None),
            content_sizes: RefCell::new(HashMap::new()),
            observed_sizes: RefCell::new(HashMap::new()),
            observed_bounds: RefCell::new(HashMap::new()),
            observed_visibility: RefCell::new(HashMap::new()),
            after_layout: RefCell::new(Vec::new()),
            layout_listeners: RefCell::new(HashMap::new()),
            mount_initialization: RefCell::new(None),
            interaction: RefCell::new(None),
            advancing_interaction: Cell::new(false),
            window_gesture: Cell::new(None),
            selection: Default::default(),
            last_pressed: Cell::new(None),
        });
        Self::from_storage(storage)
    }
    fn from_storage(storage: Rc<UiStorage>) -> Self {
        Self {
            runtime: storage.runtime.clone(),
            scene: storage.scene.clone(),
            input: storage.input.clone(),
            semantics: storage.semantics.clone(),
            storage,
        }
    }
    /// The runtime's themes: switch, load and register themes, declare app
    /// tokens (see [`crate::theme`]).
    pub fn theme(&self) -> &Rc<Themes> {
        &self.storage.themes
    }
    /// The theme scope widgets mounting now style themselves from: the
    /// enclosing `theme_scope`'s, else the root.
    pub fn theme_scope(&self) -> Rc<ThemeScope> {
        self.storage
            .mount_theme
            .borrow()
            .clone()
            .unwrap_or_else(|| self.storage.themes.root().clone())
    }
    /// Mount under `scope` until the guard drops.
    pub(crate) fn enter_theme(&self, scope: &Rc<ThemeScope>) -> ThemeGuard {
        let current = self.storage.mount_theme.borrow();
        if current
            .as_ref()
            .is_some_and(|current| Rc::ptr_eq(current, scope))
        {
            return ThemeGuard { restore: None };
        }
        drop(current);
        ThemeGuard {
            restore: Some((
                Rc::downgrade(&self.storage),
                self.storage.mount_theme.replace(Some(scope.clone())),
            )),
        }
    }
    pub(crate) fn begin_mount_initialization(&self) -> Rc<MountInitialization> {
        Rc::new(MountInitialization {
            ui: self.downgrade(),
            root: Cell::new(None),
            failed: Cell::new(false),
        })
    }
    pub(crate) fn shared(&self) -> Self {
        Self::from_storage(self.storage.clone())
    }
    pub(crate) fn selection_state(&self) -> &RefCell<crate::selection::State> {
        &self.storage.selection
    }
    /// What the last primary press went to, while it is mounted.
    pub(crate) fn last_pressed(&self) -> Option<NodeId> {
        self.storage
            .last_pressed
            .get()
            .filter(|id| self.scene.borrow().contains(*id))
    }
    /// After the document had `event`: what the selection and keyboard
    /// scrolling make of it.
    fn after_dispatch(
        &self,
        event: (
            crate::selection::SelectionEvent,
            crate::scroll_keys::ScrollKey,
            bool,
        ),
        result: &mut crate::input::DispatchResult,
    ) {
        let (selection, scroll, press) = event;
        if press {
            self.storage.last_pressed.set(result.target);
        }
        self.selection_after_dispatch(selection, result);
        self.scroll_after_dispatch(scroll, result);
    }
    /// A handle that does not keep the document alive.
    pub fn downgrade(&self) -> WeakUi {
        WeakUi {
            storage: Rc::downgrade(&self.storage),
        }
    }
    pub(crate) fn retain(&mut self, owner: NodeId, resource: impl Any) {
        self.storage
            .owned
            .borrow_mut()
            .entry(owner)
            .or_default()
            .resources
            .push(Box::new(resource));
    }
    pub fn root(&self) -> NodeId {
        self.scene.borrow().root()
    }
    pub fn signal<T: 'static>(&self, value: T) -> Signal<T> {
        self.runtime.signal(value)
    }
    pub fn container(&mut self, parent: NodeId, layout: Layout, style: Style) -> NodeId {
        let n = self
            .scene
            .borrow_mut()
            .append(parent, NodeKind::Container(layout), style);
        self.storage.owned.borrow_mut().entry(n).or_default();
        n
    }
    pub fn label(&mut self, parent: NodeId, text: impl Into<String>, style: Style) -> NodeId {
        let text = text.into();
        let theme = self.theme_scope();
        let n = self.scene.borrow_mut().append(
            parent,
            NodeKind::Text {
                text: text.clone().into(),
                color: theme.color_untracked(tok::TEXT_PRIMARY),
                font_size: theme.number_untracked(tok::FONT_SIZE_MD),
            },
            style,
        );
        self.semantics
            .borrow_mut()
            .set(n, SemanticNode::new(Role::Label, text));
        self.storage.owned.borrow_mut().entry(n).or_default();
        n
    }
    pub fn label_signal(&mut self, parent: NodeId, value: Signal<String>, style: Style) -> NodeId {
        let n = self.label(parent, "", style);
        let scene = self.scene.clone();
        let a = self.semantics.clone();
        self.bind(n, move || {
            value.with(|v| {
                scene.borrow_mut().set_text(n, v.as_str());
                a.borrow_mut().update(n, |s| s.label = v.clone());
            });
        });
        n
    }
    pub fn bind(&mut self, owner: NodeId, mut f: impl FnMut() + 'static) {
        assert!(
            self.scene.borrow().contains(owner),
            "cannot bind a removed UI node"
        );
        let mut initial = self.storage.mount_initialization.borrow().clone();
        self.runtime.clone().batch(|| {
            let effect = self.runtime.effect(move || {
                let Some(token) = initial.take() else {
                    f();
                    return;
                };
                if token.failed.get()
                    || token
                        .ui
                        .upgrade()
                        .is_none_or(|ui| !ui.scene.borrow().contains(owner))
                {
                    return;
                }
                let scope = token.enter();
                let mut guard = InitialBindingGuard {
                    token,
                    scope: Some(scope),
                    completed: false,
                };
                f();
                guard.completed = true;
            });
            self.storage
                .owned
                .borrow_mut()
                .entry(owner)
                .or_default()
                .effects
                .push(effect);
        });
    }
    pub fn on_event(
        &mut self,
        node: NodeId,
        focusable: bool,
        f: impl FnMut(&mut crate::input::EventContext) + 'static,
    ) {
        let b = if self.input.has_listeners(node) {
            self.input.listen(node, f)
        } else {
            let mut options = self.input.options(node).unwrap_or_default();
            options.focusable = focusable;
            self.input.register(node, options, f)
        };
        self.storage
            .owned
            .borrow_mut()
            .entry(node)
            .or_default()
            .bindings
            .push(b);
    }
    fn rect(&mut self, parent: NodeId, color: Color, w: f32, h: f32, x: f32, y: f32) -> NodeId {
        let mut s = self.scene.borrow_mut();
        let id = s.append(parent, NodeKind::Rect(color), fixed(w, h));
        s.set_transform(id, Transform { x, y });
        id
    }
    /// A control's root and background. The background shows `rest`, or
    /// what the returned state selects (`HOVERED`, `ACCENTED`), from the
    /// theme, and follows theme changes.
    fn control(
        &mut self,
        parent: NodeId,
        width: f32,
        height: f32,
        rest: ColorToken,
    ) -> (NodeId, NodeId, Signal<u8>) {
        let root = self.container(parent, Layout::Overlay, fixed(width, height));
        let theme = self.theme_scope();
        let bg = self.rect(root, theme.color_untracked(rest), width, height, 0., 0.);
        let state = self.signal(RESTING);
        let scene = self.scene.clone();
        let read = state.clone();
        self.bind(root, move || {
            let color = match read.get() {
                RESTING => rest,
                HOVERED => tok::BUTTON_HOVER,
                _ => tok::ACCENT,
            };
            let kind = surface(&theme, theme.color(color));
            scene.borrow_mut().set_kind(bg, kind);
        });
        (root, bg, state)
    }
    pub fn button(
        &mut self,
        parent: NodeId,
        label: impl Into<String>,
        width: f32,
        mut clicked: impl FnMut() + 'static,
    ) -> NodeId {
        let label = label.into();
        let height = self.theme_scope().number_untracked(tok::CONTROL_HEIGHT);
        let (root, _, state) = self.control(parent, width, height, tok::BUTTON_BG);
        let text = self.label(root, &label, fixed((width - 20.).max(0.), height - 8.));
        self.scene
            .borrow_mut()
            .set_transform(text, Transform { x: 10., y: 6. });
        self.semantics.borrow_mut().remove(text);
        self.semantics
            .borrow_mut()
            .set(root, SemanticNode::new(Role::Button, label));
        let focused = Rc::new(RefCell::new(false));
        self.on_event(root, true, move |cx| {
            if cx.phase != EventPhase::Target {
                return;
            }
            match cx.event {
                InputEvent::Activate if !cx.default_prevented() => clicked(),
                InputEvent::PointerEnter => {
                    state.set(HOVERED);
                }
                InputEvent::PointerLeave => {
                    state.set(if *focused.borrow() { ACCENTED } else { RESTING });
                }
                InputEvent::Focus => {
                    *focused.borrow_mut() = true;
                    state.set(ACCENTED);
                }
                InputEvent::Blur => {
                    *focused.borrow_mut() = false;
                    state.set(RESTING);
                }
                _ => {}
            }
        });
        root
    }
    pub fn checkbox(
        &mut self,
        parent: NodeId,
        label: impl Into<String>,
        value: Signal<bool>,
        width: f32,
    ) -> NodeId {
        let label = label.into();
        let height = self.theme_scope().number_untracked(tok::CONTROL_HEIGHT);
        let (root, _, state) = self.control(parent, width, height, tok::INPUT_BG);
        let text = self.label(root, "", fixed(width - 16., 28.));
        self.scene
            .borrow_mut()
            .set_transform(text, Transform { x: 8., y: 6. });
        self.semantics.borrow_mut().remove(text);
        self.semantics
            .borrow_mut()
            .set(root, SemanticNode::new(Role::CheckBox, &label));
        let scene = self.scene.clone();
        let a = self.semantics.clone();
        let v = value.clone();
        self.bind(root, move || {
            let checked = v.get();
            scene.borrow_mut().set_text(
                text,
                format!("{}  {}", if checked { "☑" } else { "☐" }, label),
            );
            a.borrow_mut().update(root, |s| s.checked = Some(checked));
        });
        self.on_event(root, true, move |cx| {
            if cx.phase != EventPhase::Target {
                return;
            }
            match cx.event {
                InputEvent::Activate if !cx.default_prevented() => {
                    value.update(|v| *v = !*v);
                }
                InputEvent::Focus => {
                    state.set(HOVERED);
                }
                InputEvent::Blur => {
                    state.set(RESTING);
                }
                _ => {}
            }
        });
        root
    }
    /// Values are normalized to the finite range: NaN becomes its minimum,
    /// infinities and out-of-range finite values clamp to the nearest endpoint.
    pub fn slider(
        &mut self,
        parent: NodeId,
        label: impl Into<String>,
        value: Signal<f32>,
        range: std::ops::RangeInclusive<f32>,
        width: f32,
    ) -> NodeId {
        let min = *range.start();
        let max = *range.end();
        assert!(
            min.is_finite() && max.is_finite() && min < max,
            "slider range must be finite and increasing"
        );
        let span = max as f64 - min as f64;
        let at_position = move |position: f32, origin: f32, width: f32| {
            let fraction = ((position as f64 - origin as f64 - 8.) / (width as f64 - 26.).max(1.))
                .clamp(0., 1.);
            (min as f64 + fraction * span).clamp(min as f64, max as f64) as f32
        };
        let theme = self.theme_scope();
        let height = theme.number_untracked(tok::CONTROL_HEIGHT);
        let (root, bg, state) = self.control(parent, width, height, tok::INPUT_BG);
        let dimensions = self.observe_content_size(root);
        let fill = self.rect(root, theme.color_untracked(tok::ACCENT), 0., 4., 8., 16.);
        let knob = self.rect(
            root,
            theme.color_untracked(tok::TEXT_PRIMARY),
            10.,
            20.,
            8.,
            8.,
        );
        self.theme_rects(
            root,
            theme,
            &[(fill, tok::ACCENT), (knob, tok::TEXT_PRIMARY)],
        );
        let mut sem = SemanticNode::new(Role::Slider, label);
        sem.min = Some(min as f64);
        sem.max = Some(max as f64);
        self.semantics.borrow_mut().set(root, sem);
        let scene = self.scene.clone();
        let a = self.semantics.clone();
        let v = value.clone();
        self.bind(root, move || {
            let raw = v.get();
            let val = if raw.is_nan() {
                min
            } else {
                raw.clamp(min, max)
            };
            if raw != val {
                v.set(val);
            }
            let (width, height) = dimensions.get();
            let x = ((val as f64 - min as f64) / span * (width as f64 - 26.).max(1.)) as f32;
            let mut s = scene.borrow_mut();
            s.set_style(bg, fixed(width, height));
            s.set_style(fill, fixed(x, 4.));
            s.set_transform(
                fill,
                Transform {
                    x: 8.,
                    y: (height - 4.).max(0.) / 2.,
                },
            );
            s.set_transform(
                knob,
                Transform {
                    x: x + 8.,
                    y: (height - 20.).max(0.) / 2.,
                },
            );
            a.borrow_mut()
                .update(root, |n| n.numeric_value = Some(val as f64));
        });
        let scene = self.scene.clone();
        let mut dragging = false;
        self.on_event(root, true, move |cx| {
            if cx.phase != EventPhase::Target {
                return;
            }
            if cx.default_prevented()
                && !matches!(
                    cx.event,
                    InputEvent::PointerUp { .. }
                        | InputEvent::PointerCancel
                        | InputEvent::Blur
                        | InputEvent::Focus
                )
            {
                return;
            }
            match &cx.event {
                InputEvent::SetNumericValue(v) if v.is_finite() => {
                    value.set((*v as f32).clamp(min, max));
                    cx.prevent_default();
                }
                InputEvent::SetValue(v) => {
                    if let Ok(v) = v.parse::<f32>()
                        && v.is_finite()
                    {
                        value.set(v.clamp(min, max));
                        cx.prevent_default();
                    }
                }
                InputEvent::Increment | InputEvent::Decrement => {
                    let direction = if matches!(cx.event, InputEvent::Increment) {
                        1.
                    } else {
                        -1.
                    };
                    value.set((value.get() + direction * (span / 100.) as f32).clamp(min, max));
                    cx.prevent_default();
                }

                InputEvent::PointerDown {
                    x,
                    button: PointerButton::Primary,
                    ..
                } => {
                    dragging = true;
                    let x = *x;
                    cx.capture_pointer();
                    let (origin, width) = slider_content_bounds(&scene.borrow(), root);
                    value.set(at_position(x, origin, width));
                }
                InputEvent::PointerMove { x, .. } if dragging => {
                    let (origin, width) = slider_content_bounds(&scene.borrow(), root);
                    value.set(at_position(*x, origin, width));
                }
                InputEvent::PointerUp {
                    button: PointerButton::Primary,
                    ..
                }
                | InputEvent::PointerCancel => {
                    dragging = false;
                    cx.release_pointer();
                }
                InputEvent::KeyDown { key, .. } => {
                    let step = (span / 100.) as f32;
                    let next = match key {
                        Key::ArrowLeft | Key::ArrowDown => Some(value.get() - step),
                        Key::ArrowRight | Key::ArrowUp => Some(value.get() + step),
                        Key::Home => Some(min),
                        Key::End => Some(max),
                        _ => None,
                    };
                    if let Some(n) = next {
                        value.set(n.clamp(min, max));
                        cx.prevent_default();
                    }
                }
                InputEvent::Focus => {
                    state.set(HOVERED);
                }
                InputEvent::Blur => {
                    dragging = false;
                    state.set(RESTING);
                }
                _ => {}
            }
        });
        root
    }
    pub fn progress(
        &mut self,
        parent: NodeId,
        label: impl Into<String>,
        value: Signal<f32>,
        width: f32,
    ) -> NodeId {
        let (root, bg, _) = self.control(parent, width, 8., tok::INPUT_BG);
        let dimensions = self.observe_content_size(root);
        let theme = self.theme_scope();
        let fill = self.rect(root, theme.color_untracked(tok::ACCENT), 0., 8., 0., 0.);
        self.theme_rects(root, theme, &[(fill, tok::ACCENT)]);
        let mut sem = SemanticNode::new(Role::Progress, label);
        sem.min = Some(0.);
        sem.max = Some(1.);
        self.semantics.borrow_mut().set(root, sem);
        let scene = self.scene.clone();
        let a = self.semantics.clone();
        self.bind(root, move || {
            let raw = value.get();
            let n = if raw.is_nan() { 0. } else { raw.clamp(0., 1.) };
            if raw != n {
                value.set(n);
            }
            let (width, height) = dimensions.get();
            let mut scene = scene.borrow_mut();
            scene.set_style(bg, fixed(width, height));
            scene.set_style(fill, fixed(width * n, height));
            a.borrow_mut()
                .update(root, |s| s.numeric_value = Some(n as f64));
        });
        root
    }
    pub fn set_disabled(&mut self, node: NodeId, disabled: bool) {
        self.input.set_disabled(&self.scene, node, disabled);
        // Cleanup and observer callbacks may remove the node or change its state.
        if !self.scene.borrow().contains(node) {
            return;
        }
        let disabled = self
            .input
            .options(node)
            .is_some_and(|options| options.disabled);
        if self.semantics.borrow().get(node).is_none() {
            self.semantics
                .borrow_mut()
                .set(node, SemanticNode::new(Role::Group, ""));
        }
        self.semantics
            .borrow_mut()
            .update(node, |n| n.disabled = disabled);
        let mut s = self.scene.borrow_mut();
        let mut effects = s.effects(node);
        effects.opacity = if disabled { 0.45 } else { 1. };
        s.set_effects(node, effects);
    }
    /// Observe allocated content dimensions without subscribing to model changes.
    /// The registration belongs to the retained node and is removed with it.
    pub(crate) fn observe_content_size(&self, node: NodeId) -> Signal<(f32, f32)> {
        if let Some(signal) = self.storage.content_sizes.borrow().get(&node) {
            return signal.clone();
        }
        self.scene.borrow_mut().prepare_layout();
        let size = allocated_content_size(&self.scene.borrow(), node);
        let signal = self.signal(size);
        self.storage
            .content_sizes
            .borrow_mut()
            .insert(node, signal.clone());
        self.storage.editor_layout_revision.set(None);
        signal
    }
    /// Register newly mounted intrinsic content without forcing a layout for
    /// every row. The sentinel is replaced by the next prepared layout.
    pub(crate) fn observe_content_size_deferred(&self, node: NodeId) -> Signal<(f32, f32)> {
        if let Some(signal) = self.storage.content_sizes.borrow().get(&node) {
            return signal.clone();
        }
        let signal = self.signal((f32::NAN, f32::NAN));
        self.storage
            .content_sizes
            .borrow_mut()
            .insert(node, signal.clone());
        self.storage.editor_layout_revision.set(None);
        signal
    }
    /// Observe the node's laid-out size (its border box), published after
    /// each layout that runs: unlike `observe_bounds`, nothing an ancestor
    /// does (scrolling, translating) costs it anything. The registration
    /// belongs to the node and goes with it.
    /// How many nodes each geometry observer list holds (content sizes,
    /// sizes, bounds, visibility): for diagnostics, and tests that what
    /// unmounts lets go.
    pub fn observer_counts(&self) -> [usize; 4] {
        [
            self.storage.content_sizes.borrow().len(),
            self.storage.observed_sizes.borrow().len(),
            self.storage.observed_bounds.borrow().len(),
            self.storage.observed_visibility.borrow().len(),
        ]
    }
    pub(crate) fn observe_size(&self, node: NodeId) -> Signal<(f32, f32)> {
        if let Some(signal) = self.storage.observed_sizes.borrow().get(&node) {
            return signal.clone();
        }
        let bounds = self.scene.borrow().layout_bounds(node);
        let signal = self.signal((bounds.width, bounds.height));
        self.storage
            .observed_sizes
            .borrow_mut()
            .insert(node, signal.clone());
        self.storage.editor_layout_revision.set(None);
        signal
    }
    /// Observe world bounds, including ancestor translations without layout work.
    /// Like content-size observations, registrations are owned by their nodes.
    pub(crate) fn observe_bounds(&self, node: NodeId) -> Signal<Rect> {
        if let Some(signal) = self.storage.observed_bounds.borrow().get(&node) {
            return signal.clone();
        }
        // Current bounds, possibly before this frame's layout: the frame's
        // layout publishes them. Laying out here, mid-mount, would measure
        // text before its inherited styles apply.
        let signal = self.signal(self.scene.borrow().bounds(node));
        self.storage
            .observed_bounds
            .borrow_mut()
            .insert(node, signal.clone());
        self.storage.bounds_geometry_revision.set(None);
        signal
    }
    /// Run `f` once after the next frame preparation lays out, before the
    /// frame is drawn, whether or not the layout changed.
    pub(crate) fn after_layout(&self, f: Box<dyn FnOnce()>) {
        self.storage.after_layout.borrow_mut().push(f);
    }
    /// Run `f` after every layout pass that changes the layout, until `node`
    /// is removed.
    pub(crate) fn on_layout(&self, node: NodeId, f: Box<dyn FnMut()>) {
        self.storage
            .layout_listeners
            .borrow_mut()
            .entry(node)
            .or_default()
            .push(Rc::new(RefCell::new(f)));
    }
    /// Whether any after-layout work ran.
    fn run_after_layout(&self) -> bool {
        let pending = std::mem::take(&mut *self.storage.after_layout.borrow_mut());
        if pending.is_empty() {
            return false;
        }
        self.runtime.batch(|| {
            for f in pending {
                f();
            }
        });
        true
    }
    fn run_layout_listeners(&self) {
        let listeners: Vec<(NodeId, LayoutListener)> = {
            let listeners = self.storage.layout_listeners.borrow();
            if listeners.is_empty() {
                return;
            }
            listeners
                .iter()
                .flat_map(|(node, list)| list.iter().map(|f| (*node, f.clone())))
                .collect()
        };
        self.runtime.batch(|| {
            for (node, listener) in listeners {
                // One an earlier listener removed stays quiet; one already
                // running (it reentered a layout pass) is not called again.
                if self.scene.borrow().contains(node)
                    && let Ok(mut f) = listener.try_borrow_mut()
                {
                    f();
                }
            }
        });
    }
    /// Inform retained media that its native window can currently present.
    /// Headless hosts default to presented; hidden/suspended hosts should set false.
    pub fn set_presented(&self, presented: bool) {
        self.storage.presented.set(presented);
    }
    pub(crate) fn observe_presentation(&self) -> Signal<bool> {
        self.storage.presented.clone()
    }
    pub(crate) fn observe_visibility(&self, node: NodeId) -> Signal<bool> {
        if let Some(signal) = self.storage.observed_visibility.borrow().get(&node) {
            return signal.clone();
        }
        let signal = self.signal(false);
        self.storage
            .observed_visibility
            .borrow_mut()
            .insert(node, signal.clone());
        self.storage.bounds_geometry_revision.set(None);
        signal
    }
    /// Prepare allocated control geometry before rendering a frame or dispatching input.
    /// Native hosts call this automatically. Headless hosts should call it before `Scene::flush`.
    pub fn prepare_frame(&self) {
        self.try_prepare_frame()
            .unwrap_or_else(|error| panic!("{error}"));
    }
    /// Settle geometry, reporting cyclic or unbounded allocation feedback instead
    /// of hanging the UI thread. At most 64 layout/notification passes are allowed.
    /// Reentrant calls defer notifications to the active outer preparation.
    /// An error does not roll back application mutations; correct the dependency
    /// and call again. Native window hosts propagate this error to `Application::run`.
    pub fn try_prepare_frame(&self) -> Result<(), LayoutFeedbackError> {
        if self.storage.preparing_frame.replace(true) {
            return Ok(());
        }
        let _preparation = FramePreparationGuard(&self.storage.preparing_frame);
        for _ in 0..64 {
            self.scene.borrow_mut().prepare_layout();
            self.input.validate_drag(&self.scene);
            let (layout_revision, geometry_revision) = {
                let scene = self.scene.borrow();
                (scene.layout_revision(), scene.geometry_revision())
            };
            let layout_changed = self.storage.editor_layout_revision.get() != Some(layout_revision);
            let geometry_changed =
                self.storage.bounds_geometry_revision.get() != Some(geometry_revision);
            if !layout_changed && !geometry_changed {
                if !self.run_after_layout() {
                    // Settled: a removed focused node's focus falls to its
                    // ancestor, unless something (a list revealing a row)
                    // focused since.
                    if let Some(id) = self.storage.refocus.take()
                        && self.input.focused().is_none()
                        && self.scene.borrow().contains(id)
                    {
                        self.input.focus(&self.scene, Some(id));
                    }
                    self.sync_selection();
                    return Ok(());
                }
                continue;
            }
            let hidden_focus = self
                .input
                .focused()
                .is_some_and(|id| !self.scene.borrow().layout_visible(id));
            if hidden_focus {
                self.input.focus(&self.scene, None);
            }
            self.input.refresh_hover(&self.scene);
            self.storage
                .editor_layout_revision
                .set(Some(layout_revision));
            self.storage
                .bounds_geometry_revision
                .set(Some(geometry_revision));
            let (sizes, bounds): (Vec<_>, Vec<_>) = {
                let scene = self.scene.borrow();
                let sizes = if layout_changed {
                    let content = self.storage.content_sizes.borrow();
                    let own = self.storage.observed_sizes.borrow();
                    content
                        .iter()
                        .filter(|(node, _)| scene.contains(**node))
                        .map(|(node, signal)| {
                            (*node, signal.clone(), allocated_content_size(&scene, *node))
                        })
                        .chain(own.iter().filter(|(node, _)| scene.contains(**node)).map(
                            |(node, signal)| {
                                let b = scene.layout_bounds(*node);
                                (*node, signal.clone(), (b.width, b.height))
                            },
                        ))
                        .collect()
                } else {
                    Vec::new()
                };
                let bounds = if geometry_changed {
                    self.storage
                        .observed_bounds
                        .borrow()
                        .iter()
                        .filter(|(node, _)| scene.contains(**node))
                        .map(|(node, signal)| (*node, signal.clone(), scene.bounds(*node)))
                        .collect()
                } else {
                    Vec::new()
                };
                (sizes, bounds)
            };
            let visibility: Vec<_> = if geometry_changed {
                let scene = self.scene.borrow();
                self.storage
                    .observed_visibility
                    .borrow()
                    .iter()
                    .filter(|(node, _)| scene.contains(**node))
                    .map(|(node, signal)| {
                        (*node, signal.clone(), scene.visible_bounds(*node).is_some())
                    })
                    .collect()
            } else {
                Vec::new()
            };
            // Publish outside every storage/scene borrow: geometry subscribers can
            // mount, dispose, translate, or reenter frame preparation.
            self.runtime.batch(|| {
                for (node, signal, size) in sizes {
                    if self.scene.borrow().contains(node) {
                        signal.set(size);
                    }
                }
                for (node, signal, visible) in visibility {
                    if self.scene.borrow().contains(node) {
                        signal.set(visible);
                    }
                }
                for (node, signal, bounds) in bounds {
                    if self.scene.borrow().contains(node) {
                        signal.set(bounds);
                    }
                }
            });
            if layout_changed {
                let editors: Vec<_> = self.storage.editors.borrow().values().cloned().collect();
                for editor in editors {
                    editor.refresh_if_resized();
                }
                self.run_layout_listeners();
            }
            self.run_after_layout();
            // Internal geometry bindings are constrained by their viewport.
            // A second pass also prepares editors mounted by virtual rows.
        }
        Err(LayoutFeedbackError { passes: 64 })
    }
    /// Recompute editor geometry after changing the font backend or display scale.
    pub(crate) fn observe_text_geometry_revision(&self) -> Signal<u64> {
        self.storage.text_geometry_revision.clone()
    }
    pub fn refresh_text_geometry(&self) {
        let revision = self.storage.text_geometry_revision.get();
        self.storage
            .text_geometry_revision
            .set(revision.wrapping_add(1));
        let old = self.storage.editor_layout.borrow_mut().take();
        drop(old);
        let editors: Vec<_> = self.storage.editors.borrow().values().cloned().collect();
        for editor in editors {
            editor.refresh();
        }
    }
    /// Whether `id` is a text editor's root (an input or text area).
    pub fn is_editor(&self, id: NodeId) -> bool {
        self.storage.editors.borrow().contains_key(&id)
    }
    pub fn focused_editor(&self) -> Option<EditorHandle> {
        self.input
            .focused()
            .and_then(|id| self.storage.editors.borrow().get(&id).cloned())
    }
    /// The editor whose root is `node`, if it is one.
    pub fn editor(&self, node: NodeId) -> Option<EditorHandle> {
        self.storage.editors.borrow().get(&node).cloned()
    }
    /// What the last press in a `View::window_drag` region asked of the
    /// window, once: the desktop host takes it after each pointer press.
    pub fn take_window_gesture(&self) -> Option<WindowGesture> {
        self.storage.window_gesture.take()
    }
    pub(crate) fn request_window_gesture(&self, gesture: WindowGesture) {
        self.storage.window_gesture.set(Some(gesture));
    }
    pub fn dispatch(&mut self, event: InputEvent) -> crate::input::DispatchResult {
        self.try_dispatch(event)
            .unwrap_or_else(|error| panic!("{error}"))
    }
    /// Dispatch after fallible geometry preparation. On allocation feedback,
    /// the event is not delivered and may be retried after repairing the model.
    pub fn try_dispatch(
        &mut self,
        event: InputEvent,
    ) -> Result<crate::input::DispatchResult, LayoutFeedbackError> {
        self.try_prepare_frame()?;
        let after = (
            crate::selection::SelectionEvent::of(&event, Default::default()),
            crate::scroll_keys::ScrollKey::of(&event),
            matches!(
                event,
                InputEvent::PointerDown {
                    button: crate::input::PointerButton::Primary,
                    ..
                }
            ),
        );
        let mut result = self.input.dispatch(&self.scene, event);
        self.after_dispatch(after, &mut result);
        Ok(result)
    }
    /// Dispatch native-style pointer input with modifiers and click counting.
    /// Ordinary `dispatch` retains unmodified, single-click pointer behavior.
    pub fn dispatch_with_modifiers(
        &mut self,
        event: InputEvent,
        modifiers: crate::input::Modifiers,
    ) -> crate::input::DispatchResult {
        self.try_dispatch_with_modifiers(event, modifiers)
            .unwrap_or_else(|error| panic!("{error}"))
    }
    /// Fallible counterpart of `dispatch_with_modifiers`; failed geometry
    /// preparation leaves the event and click sequence unprocessed.
    pub fn try_dispatch_with_modifiers(
        &mut self,
        event: InputEvent,
        modifiers: crate::input::Modifiers,
    ) -> Result<crate::input::DispatchResult, LayoutFeedbackError> {
        self.try_prepare_frame()?;
        let after = (
            crate::selection::SelectionEvent::of(&event, modifiers),
            crate::scroll_keys::ScrollKey::of(&event),
            matches!(
                event,
                InputEvent::PointerDown {
                    button: crate::input::PointerButton::Primary,
                    ..
                }
            ),
        );
        let mut result = self
            .input
            .dispatch_with_modifiers(&self.scene, event, modifiers);
        self.after_dispatch(after, &mut result);
        Ok(result)
    }
    fn interaction_valid(&self, job: &EditorInteraction) -> bool {
        job.gesture.mode.get().is_some()
            && job.gesture.generation.get() == job.generation
            && self.input.captured() == Some(job.owner)
            && self.input.focused() == Some(job.owner)
            && self.input.is_enabled(&self.scene.borrow(), job.owner)
    }
    /// Next deadline for active editor drag autoscroll. Idle editors schedule nothing.
    pub fn next_interaction_deadline(&self) -> Option<Instant> {
        [
            self.input.next_key_deadline(),
            self.next_editor_interaction_deadline(),
            self.next_selection_deadline(),
        ]
        .into_iter()
        .flatten()
        .min()
    }
    fn next_editor_interaction_deadline(&self) -> Option<Instant> {
        let pending = self
            .storage
            .interaction
            .borrow()
            .as_ref()
            .map(|p| (p.job.clone(), p.next));
        let (job, deadline) = pending?;
        let owned = self.interaction_valid(&job);
        if owned && (job.can_scroll)() {
            Some(deadline)
        } else {
            if !owned && job.gesture.generation.get() == job.generation {
                job.gesture.cancel();
            }
            let old = self.storage.interaction.borrow_mut().take();
            drop(old);
            None
        }
    }
    /// Advance due captured-editor interactions using an explicit monotonic time.
    /// Returns whether a tick ran; stalls contribute at most 50 ms of motion.
    pub fn advance_interactions(&mut self, now: Instant) -> Result<bool, LayoutFeedbackError> {
        if self.storage.advancing_interaction.get() {
            return Ok(false);
        }
        let keys = self.input.advance_key_sequence(&self.scene, now);
        if keys {
            self.try_prepare_frame()?;
        }
        // A selection drag held past a scroll viewport's edge.
        let keys = if self.advance_selection_scroll(now) {
            self.try_prepare_frame()?;
            true
        } else {
            keys
        };

        if self
            .next_editor_interaction_deadline()
            .is_none_or(|deadline| deadline > now)
        {
            return Ok(keys);
        }
        self.try_prepare_frame()?;
        // Geometry refresh can stop the gesture before the scheduled tick.
        if self
            .next_editor_interaction_deadline()
            .is_none_or(|deadline| deadline > now)
        {
            return Ok(keys);
        }
        let pending = self.storage.interaction.borrow_mut().take().unwrap();
        let elapsed = now
            .saturating_duration_since(pending.last)
            .min(Duration::from_millis(50));
        let storage = self.storage.clone();
        storage.advancing_interaction.set(true);
        let _guard = FramePreparationGuard(&storage.advancing_interaction);
        (pending.job.tick)(elapsed.as_secs_f64());
        let prepared = self.try_prepare_frame();
        let valid = self.interaction_valid(&pending.job);
        if !valid && pending.job.gesture.generation.get() == pending.job.generation {
            pending.job.gesture.cancel();
        }
        if valid && (pending.job.can_scroll)() && self.storage.interaction.borrow().is_none() {
            *self.storage.interaction.borrow_mut() = Some(InteractionDeadline {
                job: pending.job,
                last: now,
                next: now + Duration::from_millis(16),
            });
        }
        prepared?;
        Ok(true)
    }
    /// Cancel captured gestures and their deadlines, for example when a host hides.
    pub fn cancel_interactions(&mut self) {
        self.input.cancel_pending_keys();
        self.input.cancel_drag(&self.scene);
        if self.storage.interaction.borrow().is_none() && self.input.captured().is_none() {
            return;
        }
        let old = self.storage.interaction.borrow_mut().take();
        if let Some(pending) = &old {
            pending.job.gesture.cancel();
        }
        drop(old);
        self.input.dispatch(&self.scene, InputEvent::PointerCancel);
    }
    pub fn remove(&mut self, node: NodeId) {
        if !self.scene.borrow().contains(node) {
            return;
        }
        let mut ids = vec![node];
        {
            let s = self.scene.borrow();
            let mut i = 0;
            while i < ids.len() {
                ids.extend_from_slice(s.children(ids[i]));
                i += 1;
            }
        }
        let remove_interaction = self
            .storage
            .interaction
            .borrow()
            .as_ref()
            .is_some_and(|pending| ids.contains(&pending.job.owner));
        if remove_interaction {
            let old = self.storage.interaction.borrow_mut().take();
            if let Some(pending) = &old {
                pending.job.gesture.cancel();
            }
            drop(old);
        }
        let cached_layout = if self
            .storage
            .editor_layout
            .borrow()
            .as_ref()
            .is_some_and(|entry| ids.contains(&entry.node))
        {
            self.storage.editor_layout.borrow_mut().take()
        } else {
            None
        };
        // Extract all ownership before running user destructors. A destructor may
        // upgrade WeakUi and remove a sibling or the already detached subtree.
        let owned: Vec<_> = {
            let mut storage = self.storage.owned.borrow_mut();
            ids.iter().filter_map(|id| storage.remove(id)).collect()
        };
        let editors: Vec<_> = {
            let mut storage = self.storage.editors.borrow_mut();
            ids.iter().filter_map(|id| storage.remove(id)).collect()
        };
        // Focus inside it falls to its nearest focusable ancestor that stays
        // (as a page's falls to its body), so keys still reach the app: once
        // the frame settles (`try_prepare_frame`).
        let refocus = self
            .input
            .focused()
            .filter(|id| ids.contains(id))
            .and_then(|_| {
                let s = self.scene.borrow();
                let mut at = s.parent(node);
                while let Some(id) = at {
                    if self.input.options(id).is_some_and(|o| o.focusable) {
                        return Some(id);
                    }
                    at = s.parent(id);
                }
                None
            });
        // Detach geometry before listener captures or cached layouts can run
        // user destructors. Reentrant removal of this owner (or an ancestor)
        // then observes an already removed subtree instead of stale node IDs.
        self.scene.borrow_mut().remove(node);
        for id in ids {
            self.storage.content_sizes.borrow_mut().remove(&id);
            self.storage.observed_sizes.borrow_mut().remove(&id);
            self.storage.observed_bounds.borrow_mut().remove(&id);
            self.storage.observed_visibility.borrow_mut().remove(&id);
            if !self.storage.layout_listeners.borrow().is_empty() {
                // Dropped outside the borrow: a listener's captures may reenter.
                let listeners = self.storage.layout_listeners.borrow_mut().remove(&id);
                drop(listeners);
            }
            self.input.unregister(id);
            self.semantics.borrow_mut().remove(id);
        }
        drop(cached_layout);
        // A page of thousands of nodes gone: the maps give back their room.
        crate::shrink_sparse(&mut self.storage.owned.borrow_mut());
        crate::shrink_sparse(&mut self.storage.editors.borrow_mut());
        crate::shrink_sparse(&mut self.storage.content_sizes.borrow_mut());
        crate::shrink_sparse(&mut self.storage.observed_sizes.borrow_mut());
        crate::shrink_sparse(&mut self.storage.observed_bounds.borrow_mut());
        drop(owned);
        drop(editors);
        if refocus.is_some() && self.input.focused().is_none() {
            self.storage.refocus.set(refocus);
        }
    }
}
pub fn fixed(width: f32, height: f32) -> Style {
    Style {
        width: Some(width),
        height: Some(height),
        ..Default::default()
    }
}

impl Ui {
    /// Editable text with native-host clipboard/IME support. Multiline inputs use
    /// explicit line breaks by default; `EditorHandle::set_wrap` enables soft wrapping.
    pub fn text_input(
        &mut self,
        parent: NodeId,
        label: impl Into<String>,
        value: Signal<String>,
        width: f32,
        multiline: bool,
    ) -> EditorHandle {
        self.build_text_input(parent, label.into(), value, width, multiline, false)
    }
    /// Undecorated editable control used by the declarative view layer.
    pub(crate) fn text_input_styled(
        &mut self,
        parent: NodeId,
        label: impl Into<String>,
        value: Signal<String>,
        multiline: bool,
    ) -> EditorHandle {
        self.build_text_input(parent, label.into(), value, 240., multiline, true)
    }
    fn build_text_input(
        &mut self,
        parent: NodeId,
        label: String,
        value: Signal<String>,
        width: f32,
        multiline: bool,
        styled: bool,
    ) -> EditorHandle {
        // Canonical model writeback can invoke observers that remove this
        // subtree. Finish attaching owned bindings and listeners first.
        self.runtime
            .clone()
            .batch(|| self.build_text_input_inner(parent, label, value, width, multiline, styled))
    }
    fn build_text_input_inner(
        &mut self,
        parent: NodeId,
        label: String,
        value: Signal<String>,
        width: f32,
        multiline: bool,
        styled: bool,
    ) -> EditorHandle {
        let theme = self.theme_scope();
        let height = if multiline {
            156.
        } else {
            theme.number_untracked(tok::CONTROL_HEIGHT)
        };
        let (root, bg, bg_state) = if styled {
            let mut style = fixed(width, height);
            // As a CSS form control in flex: its width is a fixed intrinsic
            // one, never its text's (which scrolls inside it); a stretching
            // parent widens it, and a row too narrow for it shrinks it, to
            // nothing if need be.
            style.intrinsic_width = true;
            style.flex_shrink = 1.;
            style.padding_edges = Some(Insets {
                left: 8.,
                right: 8.,
                top: 6.,
                bottom: 6.,
            });
            (self.container(parent, Layout::Overlay, style), None, None)
        } else {
            let (root, bg, state) = self.control(parent, width, height, tok::INPUT_BG);
            (root, Some(bg), Some(state))
        };
        let mut clip_style = fixed(width - 16., height - 12.);
        clip_style.clip = true;
        let viewport = self.container(root, Layout::Overlay, clip_style);
        self.scene.borrow_mut().set_transform(
            viewport,
            if styled {
                Transform::default()
            } else {
                Transform { x: 8., y: 6. }
            },
        );
        let selection_layer = self.container(viewport, Layout::Overlay, Style::default());
        // Line numbers (`set_line_numbers`): a clipped column beside the
        // viewport, which moves right to make room.
        let gutter = self.container(root, Layout::Overlay, fixed(0., 0.));
        let number_nodes: Rc<RefCell<Vec<NodeId>>> = Rc::default();
        let line_numbers = Rc::new(Cell::new(false));
        let draw_numbers = line_numbers.clone();
        let mut initial = value.get();
        if let std::borrow::Cow::Owned(canonical) = normalize_text(&initial, multiline) {
            initial = canonical;
        }
        let text = self.label(viewport, initial.clone(), fixed(width - 16., height - 12.));
        self.semantics.borrow_mut().remove(text);
        let preedit_layer = self.container(viewport, Layout::Overlay, Style::default());
        let preedit_nodes = Rc::new(RefCell::new(Vec::new()));
        let caret = self.rect(
            viewport,
            theme.color_untracked(tok::TEXT_PRIMARY),
            1.,
            theme.number_untracked(tok::FONT_SIZE_MD) * 1.4,
            0.,
            0.,
        );
        let editor = Rc::new(RefCell::new(TextEditor::new(initial)));
        let focused = Rc::new(RefCell::new(false));
        // Blurred only because its window went to the back: still the focused field.
        let window_blurred = Rc::new(Cell::new(false));
        let draw_window_blurred = window_blurred.clone();
        let scroll = Rc::new(RefCell::new((0f32, 0f32)));
        let selection_nodes = Rc::new(RefCell::new(Vec::new()));
        self.semantics.borrow_mut().set(
            root,
            SemanticNode::new(
                if multiline {
                    Role::MultilineTextInput
                } else {
                    Role::TextInput
                },
                label,
            ),
        );
        let scene = self.scene.clone();
        let sem = self.semantics.clone();
        let edit = editor.clone();
        let focus = focused.clone();
        let scroll_for_draw = scroll.clone();
        let reveal_caret = Rc::new(Cell::new(true));
        let draw_reveal_caret = reveal_caret.clone();
        let scroll_limit = Rc::new(Cell::new((0f32, 0f32)));
        let draw_scroll_limit = scroll_limit.clone();
        let typography = Rc::new(RefCell::new((
            theme.color_untracked(tok::TEXT_PRIMARY),
            theme.number_untracked(tok::FONT_SIZE_MD),
            FontStyle::default(),
        )));
        let wrap = Rc::new(Cell::new(false));
        let preferred_x = Rc::new(Cell::new(None));
        let position = Rc::new(Cell::new(TextPosition {
            byte_offset: 0,
            affinity: TextAffinity::After,
        }));
        let draw_wrap = wrap.clone();
        let draw_position = position.clone();
        let geometry = Rc::new(RefCell::new(None));
        let draw_typography = typography.clone();
        let draw_geometry = geometry.clone();
        let draw_storage = Rc::downgrade(&self.storage);
        let read_only = Rc::new(Cell::new(false));
        let draw_read_only = read_only.clone();
        let gesture = Rc::new(EditorDrag::default());
        let draw_gesture = gesture.clone();
        let hint = Rc::new(EditorHint::default());
        let draw_hint = hint.clone();
        let refresh: Rc<dyn Fn()> = Rc::new(move || {
            let e = edit.borrow();
            let selection = e.selection();
            let mut display = std::borrow::Cow::Borrowed(e.text());
            let display_cursor = if let Some(preedit) = e.preedit() {
                let r = selection.range();
                display.to_mut().replace_range(r.clone(), &preedit.text);
                r.start
                    + preedit
                        .cursor
                        .map(|(_, end)| end)
                        .unwrap_or(preedit.text.len())
            } else {
                selection.focus
            };
            let display_cursor = display_cursor.min(display.len());
            let mut s = scene.borrow_mut();
            if !s.contains(root) {
                return;
            }
            s.prepare_layout();
            let bounds = s.bounds(root);
            let root_style = s.style(root);
            *draw_geometry.borrow_mut() = Some((
                Rect::new(0., 0., bounds.width, bounds.height),
                root_style.clone(),
                s.text_shaper_revision(),
                s.content_insets(root),
            ));
            let padding = if styled {
                s.content_insets(root)
            } else {
                Insets {
                    left: 8.,
                    right: 8.,
                    top: 6.,
                    bottom: 6.,
                }
            };
            let (color, size, font) = draw_typography.borrow().clone();
            let font = crate::text_layout::FontStyle {
                masked: draw_hint.secure.get(),
                ..font
            };
            // The gutter: as wide as the last line's number (two digits at
            // least, so the text does not move at line 10), and a gap.
            let numbered = draw_numbers.get();
            let (digits, digit) = if numbered {
                let lines = memchr_count(b'\n', display.as_bytes()) + 1;
                let digits = lines.max(10).ilog10() as usize + 1;
                (digits, s.measure_text_with_font("0", size, None, &font).0)
            } else {
                (0, 0.)
            };
            let gutter_width = if numbered {
                GUTTER_INSET + digit * digits as f32 + GUTTER_GAP
            } else {
                0.
            };
            let inner_width = (bounds.width - padding.left - padding.right - gutter_width).max(0.);
            let inner_height = (bounds.height - padding.top - padding.bottom).max(0.);
            let mut viewport_style = fixed(inner_width, inner_height);
            viewport_style.clip = true;
            s.set_style(viewport, viewport_style);
            let origin = if styled { (0., 0.) } else { (8., 6.) };
            s.set_transform(
                viewport,
                Transform {
                    x: origin.0 + gutter_width,
                    y: origin.1,
                },
            );
            let mut gutter_style = fixed(gutter_width, if numbered { inner_height } else { 0. });
            gutter_style.clip = true;
            s.set_style(gutter, gutter_style);
            s.set_transform(
                gutter,
                Transform {
                    x: origin.0,
                    y: origin.1,
                },
            );
            if let Some(bg) = bg {
                s.set_style(bg, fixed(bounds.width, bounds.height));
            }
            // Empty, the text node shows the placeholder instead (laid out
            // as the empty text it stands for: it sizes nothing).
            let placeholder = draw_hint.placeholder.borrow();
            let shown = placeholder.as_ref().filter(|_| display.is_empty());
            let (shown_text, shown_color) = match shown {
                Some((hint, hint_color)) => (
                    hint.as_ref(),
                    hint_color.unwrap_or(theme.color_untracked(tok::TEXT_TERTIARY)),
                ),
                None => (display.as_ref(), color),
            };
            // Selection and policy changes retain the same immutable paint text.
            let paint_text = match s.kind(text) {
                NodeKind::Text { text, .. } if text.as_ref() == shown_text => text.clone(),
                _ => std::sync::Arc::from(shown_text),
            };
            s.set_kind(
                text,
                NodeKind::Text {
                    text: paint_text,
                    color: shown_color,
                    font_size: size,
                },
            );
            s.set_font(text, font.clone());
            s.set_kind(caret, NodeKind::Rect(color));
            let wrap_width = draw_wrap.get().then_some(inner_width.max(1.));
            let layout = editor_layout(&draw_storage, &s, root, &display, size, wrap_width, &font);
            let prior_position = draw_position.get();
            let caret_position =
                if e.preedit().is_none() && prior_position.byte_offset == display_cursor {
                    prior_position
                } else {
                    TextPosition {
                        byte_offset: display_cursor,
                        affinity: TextAffinity::After,
                    }
                };
            let caret_rect = layout.caret_position(caret_position);
            let cx = caret_rect.x;
            let cy = caret_rect.y;
            let line_height = caret_rect.height;
            let mut scroll = scroll_for_draw.borrow_mut();
            let reveal = draw_reveal_caret.replace(true) && draw_gesture.mode.get().is_none();
            if *focus.borrow() && reveal {
                if cx - scroll.0 > (inner_width - 8.).max(0.) {
                    scroll.0 = cx - ((inner_width - 8.).max(0.));
                }
                if cx < scroll.0 {
                    scroll.0 = cx;
                }
                if cy - scroll.1 > (inner_height - line_height).max(0.) {
                    scroll.1 = cy - ((inner_height - line_height).max(0.));
                }
                if cy < scroll.1 {
                    scroll.1 = cy;
                }
            }
            let (tw, th) = layout.size();
            let limit = (
                if draw_wrap.get() {
                    0.
                } else {
                    (tw + 2. - inner_width).max(0.)
                },
                (th - inner_height).max(0.),
            );
            draw_scroll_limit.set(limit);
            scroll.0 = scroll.0.clamp(0., limit.0);
            scroll.1 = if multiline {
                scroll.1.clamp(0., limit.1)
            } else {
                // One line, centred in the box (negative: pushed down).
                (line_height - inner_height) / 2.
            };
            s.set_style(caret, fixed(1., line_height));
            let mut text_style = fixed(
                wrap_width.unwrap_or(tw.max(inner_width) + 2.),
                // A wrapped placeholder may take more lines than the text.
                if shown.is_some() {
                    th.max(inner_height)
                } else {
                    th
                },
            );
            text_style.text_wrap = draw_wrap.get();
            s.set_style(text, text_style);
            s.set_transform(
                text,
                Transform {
                    x: -scroll.0,
                    y: -scroll.1,
                },
            );
            s.set_transform(
                caret,
                Transform {
                    x: if draw_wrap.get() {
                        cx.min((inner_width - 1.).max(0.))
                    } else {
                        cx - scroll.0
                    },
                    y: cy - scroll.1,
                },
            );
            let mut effects = s.effects(caret);
            // Read-only text shows a selection but no caret.
            effects.opacity = if *focus.borrow()
                && !draw_read_only.get()
                && e.preedit().is_none_or(|preedit| preedit.cursor.is_some())
            {
                1.
            } else {
                0.
            };
            s.set_effects(caret, effects);
            // An editable field shows its selection only while it has the focus (or its
            // window merely went to the back), as AppKit's do: the selection is kept, and
            // shows again when the focus comes back. Read-only text gives its up on blur.
            let shows_selection =
                *focus.borrow() || draw_window_blurred.get() || draw_read_only.get();
            let rects: Vec<_> = if e.preedit().is_none() && shows_selection {
                layout
                    .selection(selection.range())
                    .into_iter()
                    .filter_map(|r| {
                        let y = r.y - scroll.1;
                        (y + r.height > 0. && y < inner_height).then_some((
                            r.x - scroll.0,
                            y,
                            r.width,
                            r.height,
                        ))
                    })
                    .collect()
            } else {
                Vec::new()
            };
            let mut nodes = selection_nodes.borrow_mut();
            while nodes.len() > rects.len() {
                s.remove(nodes.pop().unwrap());
            }
            while nodes.len() < rects.len() {
                nodes.push(s.append(
                    selection_layer,
                    NodeKind::Rect(theme.color_untracked(tok::SELECTION)),
                    Style::default(),
                ));
            }
            for (id, (x, y, w, h)) in nodes.iter().zip(rects) {
                s.set_style(*id, fixed(w, h));
                s.set_transform(*id, Transform { x, y });
            }
            // The numbers of the lines in view: found by where each line
            // starts (rows are in order, so the first is a binary search),
            // built only for those.
            let mut shown: Vec<(usize, f32)> = Vec::new();
            let mut current = 0;
            if numbered {
                let starts: Vec<usize> = std::iter::once(0)
                    .chain(display.match_indices('\n').map(|(at, _)| at + 1))
                    .collect();
                let top_of = |byte_offset: usize| {
                    layout
                        .caret_position(TextPosition {
                            byte_offset,
                            affinity: TextAffinity::After,
                        })
                        .y
                        - scroll.1
                };
                let first = starts
                    .partition_point(|&start| top_of(start) + line_height <= 0.)
                    .saturating_sub(1);
                for (line, &start) in starts.iter().enumerate().skip(first) {
                    let y = top_of(start);
                    if y >= inner_height {
                        break;
                    }
                    if y + line_height > 0. {
                        shown.push((line + 1, y));
                    }
                }
                current = starts.partition_point(|&start| start <= display_cursor);
            }
            let mut numbers = number_nodes.borrow_mut();
            while numbers.len() > shown.len() {
                s.remove(numbers.pop().unwrap());
            }
            while numbers.len() < shown.len() {
                numbers.push(s.append(
                    gutter,
                    NodeKind::Text {
                        text: std::sync::Arc::from(""),
                        color,
                        font_size: size,
                    },
                    Style::default(),
                ));
            }
            let (dim, bright) = (
                theme.color_untracked(tok::TEXT_TERTIARY),
                theme.color_untracked(tok::TEXT_SECONDARY),
            );
            for (id, (line, y)) in numbers.iter().zip(shown) {
                // Right-aligned by figure spaces, a digit wide in any font.
                let label = line.to_string();
                let label: String =
                    std::iter::repeat_n('\u{2007}', digits - label.len().min(digits))
                        .chain(label.chars())
                        .collect();
                let color = if line == current { bright } else { dim };
                let unchanged = matches!(
                    s.kind(*id),
                    NodeKind::Text { text, color: c, font_size }
                        if **text == *label && *c == color && *font_size == size
                );
                if !unchanged {
                    s.set_kind(
                        *id,
                        NodeKind::Text {
                            text: std::sync::Arc::from(label),
                            color,
                            font_size: size,
                        },
                    );
                    s.set_font(*id, font.clone());
                }
                s.set_style(*id, fixed(digit * digits as f32 + 1., line_height));
                s.set_transform(*id, Transform { x: GUTTER_INSET, y });
            }
            drop(numbers);
            let lines: Vec<_> = if let Some(preedit) = e.preedit() {
                let start = selection.range().start;
                layout
                    .selection(start..start + preedit.text.len())
                    .into_iter()
                    .filter_map(|r| {
                        let y = r.y + r.height - 2. - scroll.1;
                        (y >= 0. && y < inner_height).then_some((r.x - scroll.0, y, r.width))
                    })
                    .collect()
            } else {
                Vec::new()
            };
            let mut underline = preedit_nodes.borrow_mut();
            while underline.len() > lines.len() {
                s.remove(underline.pop().unwrap());
            }
            while underline.len() < lines.len() {
                underline.push(s.append(
                    preedit_layer,
                    surface(&theme, theme.color_untracked(tok::ACCENT)),
                    Style::default(),
                ));
            }
            for (id, (x, y, w)) in underline.iter().zip(lines) {
                s.set_style(*id, fixed(w, 1.));
                s.set_transform(*id, Transform { x, y });
            }
            let mut semantics = sem.borrow_mut();
            semantics.update_text_input(
                root,
                e.text(),
                (selection.anchor, selection.focus),
                draw_read_only.get(),
            );
            semantics.set_placeholder(root, placeholder.as_ref().map(|(hint, _)| hint.as_ref()));
        });
        // Geometry refresh may make an existing stationary drag scrollable.
        // The weak callback avoids refresh -> arm -> tick -> refresh ownership cycles.
        let interaction_refresh: InteractionRefresh = Rc::new(RefCell::new(None));
        let refresh: Rc<dyn Fn()> = {
            let hook = interaction_refresh.clone();
            Rc::new(move || {
                refresh();
                let arm = hook.borrow().as_ref().and_then(Weak::upgrade);
                if let Some(arm) = arm {
                    arm();
                }
            })
        };
        let handle = EditorHandle {
            multiline,
            read_only: read_only.clone(),
            caret,
            node: root,
            editor: editor.clone(),
            value: value.clone(),
            refresh: refresh.clone(),
            typography: typography.clone(),
            wrap: wrap.clone(),
            line_numbers: line_numbers.clone(),
            preferred_x: preferred_x.clone(),
            text,
            scene: self.scene.clone(),
            geometry,
            position: position.clone(),
            hint: hint.clone(),
        };
        self.storage
            .editors
            .borrow_mut()
            .insert(root, handle.clone());
        let edit = editor.clone();
        let v = value.clone();
        let draw = refresh.clone();
        let model_position = position.clone();
        let model_preferred_x = preferred_x.clone();
        let model_gesture = gesture.clone();
        let model_storage = Rc::downgrade(&self.storage);
        self.bind(root, move || {
            // Canonical model text can be compared and applied in place without
            // cloning the full document. Release the signal borrow before writeback.
            let writeback = v.with(|current| {
                let canonical = normalize_text(current, multiline);
                let changed = edit.borrow_mut().set_text_ref(canonical.as_ref());
                if changed {
                    model_gesture.cancel();
                    if let Some(storage) = model_storage.upgrade() {
                        let remove = storage
                            .interaction
                            .borrow()
                            .as_ref()
                            .is_some_and(|p| p.job.owner == root);
                        if remove {
                            let old = storage.interaction.borrow_mut().take();
                            drop(old);
                        }
                    }
                    model_preferred_x.set(None);
                    model_position.set(TextPosition {
                        byte_offset: edit.borrow().selection().focus,
                        affinity: TextAffinity::After,
                    });
                }
                match canonical {
                    std::borrow::Cow::Owned(canonical) => Some(canonical),
                    std::borrow::Cow::Borrowed(_) => None,
                }
            });
            if let Some(canonical) = writeback {
                v.set(canonical);
            }
            draw();
        });
        let scene = self.scene.clone();
        let input_storage = Rc::downgrade(&self.storage);
        let drag_select: Rc<dyn Fn()> = {
            let scene = scene.clone();
            let gesture = gesture.clone();
            let editor = editor.clone();
            let scroll = scroll.clone();
            let typography = typography.clone();
            let wrap = wrap.clone();
            let position = position.clone();
            let input_storage = input_storage.clone();
            Rc::new(move || {
                let Some(mode) = gesture.mode.get() else {
                    return;
                };
                let s = scene.borrow();
                if !s.contains(root) {
                    return;
                }
                let bounds = s.bounds(viewport);
                let offset = *scroll.borrow();
                let (_, size, font) = typography.borrow().clone();
                let mut edit = editor.borrow_mut();
                let layout = editor_layout(
                    &input_storage,
                    &s,
                    root,
                    edit.text(),
                    size,
                    wrap.get().then_some(bounds.width.max(1.)),
                    &font,
                );
                let (px, py) = gesture.pointer.get();
                // Hit the visible edge, not an arbitrarily distant text row.
                let x = (px - bounds.x).clamp(0., bounds.width.max(0.)) + offset.0;
                let y = (py - bounds.y).clamp(0., (bounds.height - 0.01).max(0.)) + offset.1;
                let hit = layout.hit_position(x, y);
                let (anchor, focus) = match mode {
                    EditorPointerSelection::Character(anchor) => (anchor, hit),
                    EditorPointerSelection::Unit { edges, line } => {
                        let current = editor_pointer_unit(
                            edit.text(),
                            layout.as_ref(),
                            hit,
                            x,
                            y,
                            line,
                            multiline,
                        );
                        if current.0.byte_offset < edges.0.byte_offset {
                            (edges.1.byte_offset, current.0)
                        } else if current.1.byte_offset > edges.1.byte_offset {
                            (edges.0.byte_offset, current.1)
                        } else {
                            (edges.0.byte_offset, edges.1)
                        }
                    }
                };
                edit.set_selection(anchor, focus.byte_offset);
                position.set(focus);
            })
        };
        let velocity: Rc<dyn Fn() -> (f32, f32)> = {
            let scene = scene.clone();
            let gesture = gesture.clone();
            let scroll = scroll.clone();
            let limits = scroll_limit.clone();
            let wrap = wrap.clone();
            Rc::new(move || {
                let s = scene.borrow();
                if !s.contains(viewport) || gesture.mode.get().is_none() {
                    return (0., 0.);
                }
                let bounds = s.bounds(viewport);
                let point = gesture.pointer.get();
                let offset = *scroll.borrow();
                let limit = limits.get();
                (
                    if wrap.get() {
                        0.
                    } else {
                        editor_drag_velocity(point.0, bounds.x, bounds.width, offset.0, limit.0)
                    },
                    if multiline {
                        editor_drag_velocity(point.1, bounds.y, bounds.height, offset.1, limit.1)
                    } else {
                        0.
                    },
                )
            })
        };
        let can_scroll: Rc<dyn Fn() -> bool> = {
            let velocity = velocity.clone();
            Rc::new(move || velocity() != (0., 0.))
        };
        let tick: Rc<dyn Fn(f64)> = {
            let scroll = scroll.clone();
            let limits = scroll_limit.clone();
            let select = drag_select.clone();
            let draw = refresh.clone();
            let reveal = reveal_caret.clone();
            Rc::new(move |elapsed| {
                let speed = velocity();
                let limit = limits.get();
                {
                    let mut offset = scroll.borrow_mut();
                    offset.0 = (f64::from(offset.0) + f64::from(speed.0) * elapsed)
                        .clamp(0., f64::from(limit.0)) as f32;
                    offset.1 = (f64::from(offset.1) + f64::from(speed.1) * elapsed)
                        .clamp(0., f64::from(limit.1)) as f32;
                }
                select();
                reveal.set(false);
                draw();
            })
        };
        let arm: Rc<dyn Fn()> = {
            let storage = input_storage.clone();
            let gesture = gesture.clone();
            Rc::new(move || {
                let Some(storage) = storage.upgrade() else {
                    return;
                };
                if storage.advancing_interaction.get() {
                    return;
                }
                // A dormant boundary gesture must not steal the one scheduler
                // slot after capture transfers to another control.
                if storage.input.captured().is_some_and(|owner| owner != root) {
                    gesture.cancel();
                    return;
                }
                if gesture.mode.get().is_none() || !can_scroll() {
                    let remove = storage
                        .interaction
                        .borrow()
                        .as_ref()
                        .is_some_and(|p| p.job.owner == root);
                    if remove {
                        let old = storage.interaction.borrow_mut().take();
                        drop(old);
                    }
                    return;
                }
                let generation = gesture.generation.get();
                if storage
                    .interaction
                    .borrow()
                    .as_ref()
                    .is_some_and(|p| p.job.owner == root && p.job.generation == generation)
                {
                    return;
                }
                let now = Instant::now();
                let old = storage
                    .interaction
                    .borrow_mut()
                    .replace(InteractionDeadline {
                        job: Rc::new(EditorInteraction {
                            owner: root,
                            generation,
                            gesture: gesture.clone(),
                            can_scroll: can_scroll.clone(),
                            tick: tick.clone(),
                        }),
                        last: now,
                        next: now + Duration::from_millis(16),
                    });
                drop(old);
            })
        };
        *interaction_refresh.borrow_mut() = Some(Rc::downgrade(&arm));
        let draw = refresh.clone();
        self.on_event(root, true, move |cx| {
            if cx.phase != EventPhase::Target {
                return;
            }
            // Capture/target hooks can cancel editing defaults. Focus and
            // pointer-release cleanup must still run after cancellation.
            if cx.default_prevented()
                && matches!(
                    cx.event,
                    InputEvent::KeyDown { .. }
                        | InputEvent::Text(_)
                        | InputEvent::ImePreedit { .. }
                        | InputEvent::ImeCommit(_)
                        | InputEvent::SetValue(_)
                        | InputEvent::SetTextSelection { .. }
                        | InputEvent::PointerDown { .. }
                        | InputEvent::PointerMove { .. }
                        | InputEvent::Scroll { .. }
                )
            {
                return;
            }
            if read_only.get() {
                let mutation = match &cx.event {
                    InputEvent::Text(_) | InputEvent::ImePreedit { .. } | InputEvent::ImeCommit(_) | InputEvent::SetValue(_) => true,
                    InputEvent::KeyDown { key, modifiers, .. } => matches!(key, Key::Backspace | Key::Delete | Key::Enter)
                        || (modifiers.primary_shortcut() && matches!(key, Key::Character(k) if k.eq_ignore_ascii_case("z") || k.eq_ignore_ascii_case("y"))),
                    _ => false,
                };
                if mutation { cx.prevent_default(); return; }
            }
            let event = cx.event.clone();
            if matches!(
                event,
                InputEvent::PointerDown { .. }
                    | InputEvent::Text(_)
                    | InputEvent::ImeCommit(_)
                    | InputEvent::ImePreedit { .. }
                    | InputEvent::SetValue(_)
                    | InputEvent::SetTextSelection { .. }
                    | InputEvent::Blur
            ) || (gesture.mode.get().is_some() && matches!(event, InputEvent::PointerMove { .. }))
            {
                preferred_x.set(None);
            }
            let mut changed = false;
            match event {
                InputEvent::Focus => {
                    *focused.borrow_mut() = true;
                    if let Some(state) = &bg_state {
                        state.set(HOVERED);
                    }
                    // Focus coming to it, not its window coming back.
                    if hint.select_on_focus.get() && !window_blurred.replace(false) {
                        let mut edit = editor.borrow_mut();
                        edit.select_all();
                        position.set(TextPosition {
                            byte_offset: edit.selection().focus,
                            affinity: TextAffinity::After,
                        });
                        // The press that focused it selects no range of its own.
                        gesture.cancel();
                    }
                }
                InputEvent::Blur => {
                    // Still focused: its window went to the back.
                    window_blurred.set(
                        input_storage
                            .upgrade()
                            .is_some_and(|storage| storage.input.focused() == Some(root)),
                    );
                    *focused.borrow_mut() = false;
                    gesture.cancel();
                    editor.borrow_mut().cancel_preedit();
                    // Read-only text gives up its selection with the focus, as a
                    // selectable label does, unless only its window went back.
                    if read_only.get() && !window_blurred.get() {
                        let mut edit = editor.borrow_mut();
                        let end = edit.selection().focus;
                        edit.set_selection(end, end);
                    }
                    if let Some(state) = &bg_state {
                        state.set(RESTING);
                    }
                }
                InputEvent::Scroll {
                    delta_x, delta_y, ..
                } => {
                    if delta_x.is_finite() && delta_y.is_finite() {
                        let mut offset = scroll.borrow_mut();
                        let previous = *offset;
                        let limit = scroll_limit.get();
                        offset.0 = (offset.0 + delta_x).clamp(0., limit.0);
                        if multiline {
                            offset.1 = (offset.1 + delta_y).clamp(0., limit.1);
                        }
                        if previous == *offset {
                            return;
                        }
                        drop(offset);
                        reveal_caret.set(false);
                        cx.prevent_default();
                        cx.stop_propagation();
                    }
                }
                InputEvent::Text(t) => {
                    let canonical = normalize_text(&t, multiline);
                    if !canonical
                        .chars()
                        .any(|c| c.is_control() && c != '\n' && c != '\t')
                    {
                        changed = editor.borrow_mut().insert(&canonical);
                    }
                }
                InputEvent::ImePreedit { text, cursor } => {
                    editor.borrow_mut().set_preedit(text, cursor)
                }
                InputEvent::SetValue(t) => {
                    let t = normalize_text(&t, multiline);
                    changed = editor.borrow_mut().set_text_ref(&t);
                    cx.prevent_default();
                }
                InputEvent::SetTextSelection { anchor, focus } => {
                    let mut edit = editor.borrow_mut();
                    edit.set_selection(anchor, focus);
                    position.set(TextPosition {
                        byte_offset: edit.selection().focus,
                        affinity: TextAffinity::After,
                    });
                    cx.prevent_default();
                }
                InputEvent::ImeCommit(t) => {
                    changed = editor
                        .borrow_mut()
                        .commit_preedit(&normalize_text(&t, multiline));
                }
                InputEvent::PointerDown {
                    x,
                    y,
                    button: PointerButton::Primary,
                } => {
                    cx.capture_pointer();
                    gesture.pointer.set((x, y));
                    let s = scene.borrow();
                    let bounds = s.bounds(viewport);
                    let scroll = *scroll.borrow();
                    let (_, size, font) = typography.borrow().clone();
                    let mut edit = editor.borrow_mut();
                    let layout = editor_layout(
                        &input_storage, &s, root, edit.text(), size,
                        wrap.get().then_some(bounds.width.max(1.)), &font,
                    );
                    let x = x - bounds.x + scroll.0;
                    let y = y - bounds.y + scroll.1;
                    let hit = layout.hit_position(x, y);
                    let mode = if cx.pointer_modifiers().shift {
                        EditorPointerSelection::Character(edit.selection().anchor)
                    } else {
                        match cx.click_count() {
                            2 => {
                                let edges = editor_pointer_unit(edit.text(), layout.as_ref(), hit, x, y, false, multiline);
                                EditorPointerSelection::Unit { edges, line: false }
                            }
                            3 => {
                                let edges = editor_pointer_unit(edit.text(), layout.as_ref(), hit, x, y, true, multiline);
                                EditorPointerSelection::Unit { edges, line: true }
                            }
                            _ => EditorPointerSelection::Character(hit.byte_offset),
                        }
                    };
                    let (anchor, focus) = match mode {
                        EditorPointerSelection::Character(anchor) => (anchor, hit),
                        EditorPointerSelection::Unit { edges, .. } => (edges.0.byte_offset, edges.1),
                    };
                    edit.set_selection(anchor, focus.byte_offset);
                    position.set(focus);
                    gesture.cancel();
                    gesture.mode.set(Some(mode));
                }
                InputEvent::PointerMove { x, y } if gesture.mode.get().is_some() => {
                    gesture.pointer.set((x, y));
                    drag_select();
                    reveal_caret.set(false);
                }
                InputEvent::PointerUp {
                    button: PointerButton::Primary,
                    ..
                }
                | InputEvent::PointerCancel => {
                    gesture.cancel();
                    cx.release_pointer();
                }
                InputEvent::KeyUp {
                    key: Key::Space, ..
                } => {
                    // Let keydown deliver native text; suppress the dispatcher's
                    // delayed keyboard activation when the space key is released.
                    cx.prevent_default();
                    return;
                }
                InputEvent::KeyDown { key, modifiers, .. } => {
                    let paging = matches!(key, Key::PageUp | Key::PageDown);
                    // Single-line fields and platform shortcuts leave paging to
                    // their ancestors without disturbing editor navigation state.
                    if paging && (!multiline || modifiers.control || modifiers.alt || modifiers.meta) {
                        return;
                    }
                    let mut e = editor.borrow_mut();
                    let command = modifiers.primary_shortcut();
                    let word = modifiers.word_navigation();
                    let visual_edge = match key {
                        Key::Home | Key::End if !command && wrap.get() => Some(key == Key::End),
                        Key::ArrowLeft | Key::ArrowRight
                            if cfg!(target_os = "macos") && modifiers.meta && wrap.get() =>
                        {
                            Some(key == Key::ArrowRight)
                        }
                        _ => None,
                    };
                    let directional = paging || (matches!(key, Key::ArrowUp | Key::ArrowDown)
                        && !(cfg!(target_os = "macos") && modifiers.meta));
                    if !directional {
                        preferred_x.set(None);
                    }
                    if let Some(end) = visual_edge {
                        let s = scene.borrow();
                        let (_, size, font) = typography.borrow().clone();
                        let layout = editor_layout(
                            &input_storage,
                            &s,
                            root,
                            e.text(),
                            size,
                            Some(s.bounds(viewport).width.max(1.)),
                            &font,
                        );
                        let old = position.get();
                        let current = if old.byte_offset == e.selection().focus {
                            old
                        } else {
                            TextPosition {
                                byte_offset: e.selection().focus,
                                affinity: TextAffinity::After,
                            }
                        };
                        let next = layout.visual_line_edge(current, end);
                        let anchor = if modifiers.shift {
                            e.selection().anchor
                        } else {
                            next.byte_offset
                        };
                        e.set_selection(anchor, next.byte_offset);
                        position.set(next);
                    } else {
                        if !directional {
                            position.set(TextPosition {
                                byte_offset: e.selection().focus,
                                affinity: TextAffinity::After,
                            });
                        }
                        match key {
                            Key::ArrowLeft if cfg!(target_os = "macos") && modifiers.meta => {
                                e.move_home(modifiers.shift)
                            }
                            Key::ArrowRight if cfg!(target_os = "macos") && modifiers.meta => {
                                e.move_end(modifiers.shift)
                            }
                            Key::ArrowUp if cfg!(target_os = "macos") && modifiers.meta => {
                                e.move_document_start(modifiers.shift)
                            }
                            Key::ArrowDown if cfg!(target_os = "macos") && modifiers.meta => {
                                e.move_document_end(modifiers.shift)
                            }
                            Key::ArrowLeft => e.move_left(word, modifiers.shift),
                            Key::ArrowRight => e.move_right(word, modifiers.shift),
                            Key::Home => {
                                if command {
                                    e.move_document_start(modifiers.shift)
                                } else {
                                    e.move_home(modifiers.shift)
                                }
                            }
                            Key::End => {
                                if command {
                                    e.move_document_end(modifiers.shift)
                                } else {
                                    e.move_end(modifiers.shift)
                                }
                            }
                            Key::ArrowUp | Key::ArrowDown | Key::PageUp | Key::PageDown => {
                                let direction = if matches!(key, Key::ArrowUp | Key::PageUp) { -1. } else { 1. };
                                let (_, size, font) = typography.borrow().clone();
                                let s = scene.borrow();
                                let layout = editor_layout(
                                    &input_storage,
                                    &s,
                                    root,
                                    e.text(),
                                    size,
                                    wrap.get().then_some(s.bounds(viewport).width.max(1.)),
                                    &font,
                                );
                                let old = position.get();
                                let current = if old.byte_offset == e.selection().focus {
                                    old
                                } else {
                                    TextPosition {
                                        byte_offset: e.selection().focus,
                                        affinity: TextAffinity::After,
                                    }
                                };
                                let caret = layout.caret_position(current);
                                let desired_x = if old.byte_offset != e.selection().focus {
                                    caret.x
                                } else {
                                    preferred_x.get().unwrap_or(caret.x)
                                };
                                preferred_x.set(Some(desired_x));
                                let next = if paging {
                                    // One visual line overlaps between pages. Use f64
                                    // arithmetic so tiny positive line pitches cannot
                                    // overflow the line count or displacement.
                                    let pitch = if caret.height.is_finite() && caret.height > 0. {
                                        f64::from(caret.height)
                                    } else { 1. };
                                    let height = s.bounds(viewport).height;
                                    let height = if height.is_finite() { f64::from(height.max(0.)) } else { pitch };
                                    let lines = ((height / pitch).floor() - 1.).max(1.);
                                    let origin = if caret.y.is_finite() { f64::from(caret.y) } else { 0. };
                                    let target = origin + f64::from(direction) * lines * pitch;
                                    let end = TextPosition { byte_offset: e.text().len(), affinity: TextAffinity::After };
                                    let last_y = layout.caret_position(end).y;
                                    if target < 0. {
                                        TextPosition::default()
                                    } else if last_y.is_finite() && target > f64::from(last_y) {
                                        end
                                    } else {
                                        layout.hit_position(desired_x,
                                            (target + pitch * 0.5).clamp(0., f64::from(f32::MAX)) as f32)
                                    }
                                } else {
                                    layout.hit_position(
                                        desired_x,
                                        (caret.y + direction * caret.height).max(0.)
                                            + caret.height * 0.5,
                                    )
                                };
                                if paging {
                                    let next_y = layout.caret_position(next).y;
                                    if next_y.is_finite() && caret.y.is_finite() {
                                        let mut offset = scroll.borrow_mut();
                                        offset.1 = (f64::from(offset.1) + f64::from(next_y) - f64::from(caret.y))
                                            .clamp(0., f64::from(scroll_limit.get().1.max(0.))) as f32;
                                    }
                                }
                                position.set(next);
                                let index = next.byte_offset;
                                let anchor = if modifiers.shift {
                                    e.selection().anchor
                                } else {
                                    index
                                };
                                e.set_selection(anchor, index);
                            }
                            Key::Backspace => {
                                changed = e.delete_backward(word);
                            }
                            Key::Delete => {
                                changed = e.delete_next(word);
                            }
                            Key::Enter => {
                                if multiline {
                                    changed = e.insert("\n");
                                }
                            }
                            Key::Space => return,
                            Key::Character(ref k) if command && k.eq_ignore_ascii_case("a") => {
                                e.select_all()
                            }
                            Key::Character(ref k) if command && k.eq_ignore_ascii_case("z") => {
                                changed = if modifiers.shift { e.redo() } else { e.undo() };
                            }
                            Key::Character(ref k) if command && k.eq_ignore_ascii_case("y") => {
                                changed = e.redo();
                            }
                            Key::Escape => e.cancel_preedit(),
                            _ => return,
                        }
                    }
                    cx.prevent_default();
                }
                _ => return,
            }
            if changed {
                position.set(TextPosition {
                    byte_offset: editor.borrow().selection().focus,
                    affinity: TextAffinity::After,
                });
                let next = editor.borrow().text().to_owned();
                value.set(next);
            }
            draw();
            arm();
        });
        handle
    }
}

impl Ui {
    /// Route an assistive-technology value change through the widget's normal input guards.
    /// Returns whether the request was handled, including a read-only rejection;
    /// it does not indicate that the model changed.
    pub fn set_accessible_value(&mut self, node: NodeId, value: &str) -> bool {
        self.input
            .dispatch_to(&self.scene, node, InputEvent::SetValue(value.to_owned()))
            .default_prevented
    }
    /// Change committed-text selection through normal input and modal guards.
    /// Editor controls clamp offsets to grapheme boundaries and cancel preedit.
    pub fn set_accessible_text_selection(
        &mut self,
        node: NodeId,
        anchor: usize,
        focus: usize,
    ) -> bool {
        self.input
            .dispatch_to(
                &self.scene,
                node,
                InputEvent::SetTextSelection { anchor, focus },
            )
            .default_prevented
    }
    pub fn set_accessible_numeric_value(&mut self, node: NodeId, value: f64) -> bool {
        self.input
            .dispatch_to(&self.scene, node, InputEvent::SetNumericValue(value))
            .default_prevented
    }
}

fn editor_drag_velocity(pointer: f32, start: f32, extent: f32, offset: f32, limit: f32) -> f32 {
    if !pointer.is_finite() || !start.is_finite() || !extent.is_finite() || extent <= 0. {
        return 0.;
    }
    let distance = if pointer < start {
        pointer - start
    } else if pointer > start + extent {
        pointer - start - extent
    } else {
        0.
    };
    if (distance < 0. && offset > 0.) || (distance > 0. && offset < limit) {
        distance.signum() * (distance.abs() * 12.).clamp(24., 1200.)
    } else {
        0.
    }
}

#[derive(Clone, Copy)]
enum EditorPointerSelection {
    Character(usize),
    Unit {
        edges: (TextPosition, TextPosition),
        line: bool,
    },
}

pub(crate) fn editor_pointer_unit(
    text: &str,
    layout: &dyn TextLayout,
    hit: TextPosition,
    x: f32,
    y: f32,
    line: bool,
    multiline: bool,
) -> (TextPosition, TextPosition) {
    if line && multiline {
        return layout.visual_line_range(hit);
    }
    let (start, end) = if line {
        (0, text.len())
    } else {
        // A hit is the nearest caret, not necessarily the grapheme underneath
        // the pointer. Check the preceding cell, including RTL geometry, before
        // choosing a Unicode word-bound segment at that caret.
        let offset = hit.byte_offset.min(text.len());
        let previous = text
            .grapheme_indices(true)
            .take_while(|(i, _)| *i < offset)
            .last();
        let index = previous
            .filter(|(start, grapheme)| {
                layout
                    .selection(*start..start + grapheme.len())
                    .iter()
                    .any(|r| x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height)
            })
            .map_or(offset, |(start, _)| start);
        editor_word_segment(text, index)
    };
    (
        TextPosition {
            byte_offset: start,
            affinity: TextAffinity::After,
        },
        TextPosition {
            byte_offset: end,
            affinity: TextAffinity::Before,
        },
    )
}

fn editor_word_segment(text: &str, index: usize) -> (usize, usize) {
    let (start, segment) = text
        .split_word_bound_indices()
        .find(|(start, segment)| index < start + segment.len())
        .or_else(|| text.split_word_bound_indices().next_back())
        .unwrap_or((0, ""));
    let end = start + segment.len();
    let start = text
        .grapheme_indices(true)
        .map(|(i, _)| i)
        .take_while(|i| *i <= start)
        .last()
        .unwrap_or(0);
    let end = text
        .grapheme_indices(true)
        .map(|(i, _)| i)
        .find(|i| *i >= end)
        .unwrap_or(text.len());
    (start, end)
}

fn normalize_text(text: &str, multiline: bool) -> std::borrow::Cow<'_, str> {
    use std::borrow::Cow;
    if !text.contains('\r') && (multiline || !text.contains('\n')) {
        return Cow::Borrowed(text);
    }
    let mut canonical = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                if multiline {
                    canonical.push('\n');
                }
            }
            '\n' if !multiline => {}
            other => canonical.push(other),
        }
    }
    Cow::Owned(canonical)
}

/// A legacy control's background states (`Ui::control`).
const RESTING: u8 = 0;
const HOVERED: u8 = 1;
const ACCENTED: u8 = 2;
impl Ui {
    /// Keep plain rectangles' colours on their tokens as the theme changes.
    fn theme_rects(
        &mut self,
        owner: NodeId,
        theme: Rc<ThemeScope>,
        rects: &[(NodeId, ColorToken)],
    ) {
        let rects = rects.to_vec();
        let scene = self.scene.clone();
        let mut first = true;
        self.bind(owner, move || {
            let colors: Vec<_> = rects.iter().map(|(_, t)| theme.color(*t)).collect();
            if std::mem::take(&mut first) {
                return;
            }
            let mut scene = scene.borrow_mut();
            for ((node, _), color) in rects.iter().zip(colors) {
                if scene.contains(*node) {
                    scene.set_kind(*node, NodeKind::Rect(color));
                }
            }
        });
    }
}
fn surface(theme: &ThemeScope, color: Color) -> NodeKind {
    NodeKind::Quad(crate::scene::QuadStyle {
        fill: color,
        radius: theme.number(tok::RADIUS_MD),
        border_color: Color(255, 255, 255, 18),
        border_width: 1.,
        shadow: None,
        decoration: None,
        border_over: false,
        outline: None,
    })
}
impl Ui {
    pub fn image(
        &mut self,
        parent: NodeId,
        image: std::sync::Arc<crate::image::ImageData>,
        alt: impl Into<String>,
        style: Style,
    ) -> NodeId {
        let n = self
            .scene
            .borrow_mut()
            .append(parent, NodeKind::Image(image), style);
        self.storage.owned.borrow_mut().entry(n).or_default();
        self.semantics
            .borrow_mut()
            .set(n, SemanticNode::new(Role::Image, alt));
        n
    }
    pub fn quad(&mut self, parent: NodeId, quad: crate::scene::QuadStyle, style: Style) -> NodeId {
        let n = self
            .scene
            .borrow_mut()
            .append(parent, NodeKind::Quad(quad), style);
        self.storage.owned.borrow_mut().entry(n).or_default();
        n
    }
}

#[cfg(test)]
mod ownership_tests {
    use super::*;
    use std::cell::Cell;

    struct OnDrop(Option<Box<dyn FnOnce()>>);
    impl Drop for OnDrop {
        fn drop(&mut self) {
            self.0.take().unwrap()();
        }
    }

    #[test]
    fn shared_handles_see_editors_and_preserve_bindings() {
        let mut ui = Ui::new(300., 200.);
        let root = ui.root();
        let value = ui.signal(String::from("initial"));
        let editor = ui.text_input(root, "Name", value.clone(), 200., false);
        let shared = ui.shared();
        assert!(ui.input.focus(&ui.scene, Some(editor.node)));
        assert_eq!(shared.focused_editor().unwrap().node, editor.node);
        drop(ui);
        value.set("still mounted".into());
        assert_eq!(
            shared.focused_editor().unwrap().editor.borrow().text(),
            "still mounted"
        );
        assert!(shared.input.has_listeners(editor.node));
    }

    #[test]
    fn only_last_owner_disposes_resources_and_effects() {
        let mut ui = Ui::new(100., 100.);
        let node = ui.label(ui.root(), "retained", Style::default());
        let runtime = ui.runtime.clone();
        let input = ui.input.clone();
        ui.bind(node, || {});
        ui.on_event(node, true, |_| {});
        let count = Rc::new(Cell::new(0));
        let c = count.clone();
        ui.retain(node, OnDrop(Some(Box::new(move || c.set(c.get() + 1)))));
        let weak = ui.downgrade();
        let shared = ui.shared();
        drop(ui);
        assert_eq!(count.get(), 0);
        assert_eq!(runtime.effect_count(), 1);
        assert!(weak.upgrade().is_some());
        drop(shared);
        assert_eq!(count.get(), 1);
        assert_eq!(runtime.effect_count(), 0);
        assert!(!input.has_listeners(node));
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn listener_destructor_can_remove_its_owner_and_ancestor() {
        let mut ui = Ui::new(100., 100.);
        let parent = ui.container(ui.root(), Layout::Column, Style::default());
        let child = ui.container(parent, Layout::Column, Style::default());
        let weak = ui.downgrade();
        let dropped = Rc::new(Cell::new(0));
        let count = dropped.clone();
        let resource = OnDrop(Some(Box::new(move || {
            count.set(count.get() + 1);
            let mut ui = weak.upgrade().unwrap();
            ui.remove(child);
            ui.remove(parent);
        })));
        ui.on_event(child, false, move |_| {
            let _ = &resource;
        });
        ui.remove(child);
        assert_eq!(dropped.get(), 1);
        assert!(!ui.scene.borrow().contains(parent));
        assert!(!ui.scene.borrow().contains(child));
        assert!(!ui.input.has_listeners(child));
        assert!(ui.storage.owned.borrow().is_empty());
    }

    #[test]
    fn resource_cleanup_can_remove_other_owned_subtrees() {
        let mut ui = Ui::new(100., 100.);
        let first = ui.container(ui.root(), Layout::Column, Style::default());
        let second = ui.container(ui.root(), Layout::Column, Style::default());
        let count = Rc::new(Cell::new(0));
        let c = count.clone();
        ui.retain(second, OnDrop(Some(Box::new(move || c.set(c.get() + 1)))));
        let weak = ui.downgrade();
        ui.retain(
            first,
            OnDrop(Some(Box::new(move || {
                let mut ui = weak.upgrade().unwrap();
                ui.remove(first);
                ui.remove(second);
            }))),
        );
        ui.remove(first);
        assert_eq!(count.get(), 1);
        assert!(!ui.scene.borrow().contains(first));
        assert!(!ui.scene.borrow().contains(second));
        assert!(ui.storage.owned.borrow().is_empty());
    }
}

fn allocated_content_size(scene: &Scene, node: NodeId) -> (f32, f32) {
    // Its own box: a size, whatever moved it.
    let bounds = scene.layout_bounds(node);
    let padding = scene.content_insets(node);
    (
        (bounds.width - padding.left - padding.right).max(0.),
        (bounds.height - padding.top - padding.bottom).max(0.),
    )
}

#[cfg(test)]
mod content_size_observer_tests {
    use super::*;

    #[test]
    fn resize_subscriber_can_remove_itself_and_another_observed_subtree() {
        let mut ui = Ui::new(400., 200.);
        let first = ui.container(ui.root(), Layout::Overlay, fixed(100., 40.));
        let second = ui.container(ui.root(), Layout::Overlay, fixed(100., 40.));
        let first_size = ui.observe_content_size(first);
        let second_size = ui.observe_content_size(second);
        let weak = ui.downgrade();
        ui.bind(first, move || {
            if first_size.get().0 > 100. {
                let mut ui = weak.upgrade().unwrap();
                ui.remove(first);
                ui.remove(second);
            }
        });
        let second_updates = Rc::new(Cell::new(0));
        let updates = second_updates.clone();
        ui.bind(second, move || {
            second_size.get();
            updates.set(updates.get() + 1);
        });
        ui.scene.borrow_mut().set_style(first, fixed(150., 40.));
        ui.scene.borrow_mut().set_style(second, fixed(150., 40.));
        ui.prepare_frame();
        assert!(!ui.scene.borrow().contains(first));
        assert!(!ui.scene.borrow().contains(second));
        assert!(ui.storage.content_sizes.borrow().is_empty());
        assert_eq!(ui.runtime.effect_count(), 0);
        assert_eq!(second_updates.get(), 1);
        ui.prepare_frame();
    }

    #[test]
    fn resize_subscriber_can_mount_observer_and_reenter_frame_preparation() {
        let mut ui = Ui::new(400., 200.);
        let first = ui.container(ui.root(), Layout::Overlay, fixed(100., 40.));
        let first_size = ui.observe_content_size(first);
        let created = Rc::new(RefCell::new(None));
        let created_output = created.clone();
        let weak = ui.downgrade();
        ui.bind(first, move || {
            if first_size.get().0 > 100. && created_output.borrow().is_none() {
                let mut ui = weak.upgrade().unwrap();
                let child = ui.container(first, Layout::Overlay, fixed(30., 20.));
                let size = ui.observe_content_size(child);
                *created_output.borrow_mut() = Some((child, size));
                ui.scene.borrow_mut().set_style(child, fixed(70., 25.));
                ui.prepare_frame();
            }
        });
        ui.scene.borrow_mut().set_style(first, fixed(150., 40.));
        ui.prepare_frame();
        let (child, size) = created.borrow().as_ref().unwrap().clone();
        assert_eq!(size.get(), (70., 25.));
        assert_eq!(ui.storage.content_sizes.borrow().len(), 2);
        ui.remove(first);
        assert!(!ui.scene.borrow().contains(child));
        assert!(ui.storage.content_sizes.borrow().is_empty());
        ui.prepare_frame();
    }
}

#[cfg(test)]
mod layout_feedback_tests {
    use super::*;

    fn feedback_can_be_repaired(next: fn(f32) -> f32) {
        let mut ui = Ui::new(1000., 200.);
        let node = ui.container(ui.root(), Layout::Overlay, fixed(100., 40.));
        let size = ui.observe_content_size(node);
        let enabled = Rc::new(Cell::new(false));
        let active = enabled.clone();
        let updates = Rc::new(Cell::new(0));
        let count = updates.clone();
        let scene = ui.scene.clone();
        ui.bind(node, move || {
            let width = size.get().0;
            if active.get() {
                count.set(count.get() + 1);
                assert!(
                    count.get() < 200,
                    "frame preparation failed to bound feedback"
                );
                scene.borrow_mut().set_style(node, fixed(next(width), 40.));
            }
        });
        ui.try_prepare_frame().unwrap();
        enabled.set(true);
        ui.scene.borrow_mut().set_style(node, fixed(101., 40.));
        assert!(ui.try_prepare_frame().is_err());
        assert!(updates.get() > 1);
        enabled.set(false);
        ui.scene.borrow_mut().set_style(node, fixed(150., 40.));
        ui.try_prepare_frame().unwrap();
        let previous = updates.get();
        ui.try_prepare_frame().unwrap();
        assert_eq!(previous, updates.get());
        assert_eq!(ui.scene.borrow().bounds(node).width, 150.);
    }

    #[test]
    fn alternating_allocation_feedback_returns_error_and_recovers() {
        feedback_can_be_repaired(|width| if width == 101. { 102. } else { 101. });
    }

    #[test]
    fn monotonically_growing_allocation_feedback_returns_error_and_recovers() {
        feedback_can_be_repaired(|width| width + 1.);
    }

    #[test]
    fn reentrant_preparation_defers_to_outer_frame_until_allocation_settles() {
        let mut ui = Ui::new(400., 200.);
        let node = ui.container(ui.root(), Layout::Overlay, fixed(100., 40.));
        let size = ui.observe_content_size(node);
        let weak = ui.downgrade();
        let observed = Rc::new(Cell::new(0.));
        let latest = observed.clone();
        ui.bind(node, move || {
            let width = size.get().0;
            latest.set(width);
            if width == 150. {
                let ui = weak.upgrade().unwrap();
                ui.scene.borrow_mut().set_style(node, fixed(180., 40.));
                // Reactive effects are already flushing. A nested preparation
                // must not spin waiting for this callback's own next delivery.
                ui.try_prepare_frame().unwrap();
            }
        });
        ui.scene.borrow_mut().set_style(node, fixed(150., 40.));
        ui.try_prepare_frame().unwrap();
        assert_eq!(observed.get(), 180.);
        assert_eq!(ui.scene.borrow().bounds(node).width, 180.);
    }

    #[test]
    fn panicking_resize_callback_does_not_disable_future_preparation() {
        let mut ui = Ui::new(400., 200.);
        let node = ui.container(ui.root(), Layout::Overlay, fixed(100., 40.));
        let size = ui.observe_content_size(node);
        let fail = Rc::new(Cell::new(true));
        let panic_once = fail.clone();
        let observed = Rc::new(Cell::new(0.));
        let latest = observed.clone();
        ui.bind(node, move || {
            let width = size.get().0;
            if width > 100. && panic_once.replace(false) {
                panic!("resize callback failed");
            }
            latest.set(width);
        });
        ui.scene.borrow_mut().set_style(node, fixed(150., 40.));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ui.prepare_frame()));
        assert!(result.is_err());
        ui.scene.borrow_mut().set_style(node, fixed(180., 40.));
        ui.try_prepare_frame().unwrap();
        assert_eq!(observed.get(), 180.);
    }
}

fn slider_content_bounds(scene: &Scene, root: NodeId) -> (f32, f32) {
    let bounds = scene.bounds(root);
    let padding = scene.content_insets(root);
    (
        bounds.x + padding.left,
        (bounds.width - padding.left - padding.right).max(0.),
    )
}

#[cfg(test)]
mod bounds_observer_tests {
    use super::*;

    #[test]
    fn world_bounds_follow_ancestor_translation_without_layout_and_idle_notifications() {
        let mut ui = Ui::new(400., 200.);
        let parent = ui.container(ui.root(), Layout::Overlay, fixed(100., 80.));
        let child = ui.container(parent, Layout::Overlay, fixed(30., 20.));
        let bounds = ui.observe_bounds(child);
        let updates = Rc::new(Cell::new(0));
        let count = updates.clone();
        let observed = bounds.clone();
        ui.bind(child, move || {
            observed.get();
            count.set(count.get() + 1);
        });
        // Observing does not lay out mid-mount: the first frame publishes the
        // laid-out bounds.
        ui.prepare_frame();
        ui.scene.borrow_mut().flush();
        assert_eq!(bounds.get(), Rect::new(0., 0., 30., 20.));
        assert_eq!(updates.get(), 2);
        ui.scene
            .borrow_mut()
            .set_transform(parent, Transform { x: 12., y: 34. });
        ui.prepare_frame();
        assert_eq!(bounds.get(), Rect::new(12., 34., 30., 20.));
        assert_eq!(updates.get(), 3);
        assert_eq!(ui.scene.borrow_mut().flush().layout_nodes, 0);
        ui.prepare_frame();
        assert_eq!(updates.get(), 3);
        ui.scene.borrow_mut().set_style(child, fixed(45., 25.));
        ui.prepare_frame();
        assert_eq!(bounds.get(), Rect::new(12., 34., 45., 25.));
        assert_eq!(updates.get(), 4);
        ui.remove(parent);
        assert!(ui.storage.observed_bounds.borrow().is_empty());
        assert_eq!(ui.runtime.effect_count(), 0);
    }

    #[test]
    fn bounds_subscriber_can_dispose_itself_and_reenter_preparation() {
        let mut ui = Ui::new(400., 200.);
        let node = ui.container(ui.root(), Layout::Overlay, fixed(100., 40.));
        let bounds = ui.observe_bounds(node);
        let weak = ui.downgrade();
        ui.bind(node, move || {
            if bounds.get().x > 0. {
                let mut ui = weak.upgrade().unwrap();
                ui.remove(node);
                ui.prepare_frame();
            }
        });
        ui.scene
            .borrow_mut()
            .set_transform(node, Transform { x: 1., y: 0. });
        ui.prepare_frame();
        assert!(!ui.scene.borrow().contains(node));
        assert!(ui.storage.observed_bounds.borrow().is_empty());
        assert_eq!(ui.runtime.effect_count(), 0);
    }

    #[test]
    fn transform_feedback_is_bounded_and_recovers_after_disposal() {
        let mut ui = Ui::new(400., 200.);
        let node = ui.container(ui.root(), Layout::Overlay, fixed(100., 40.));
        let bounds = ui.observe_bounds(node);
        let scene = Rc::downgrade(&ui.scene);
        ui.bind(node, move || {
            let x = bounds.get().x;
            scene
                .upgrade()
                .unwrap()
                .borrow_mut()
                .set_transform(node, Transform { x: x + 1., y: 0. });
        });
        assert_eq!(ui.try_prepare_frame().unwrap_err().passes, 64);
        ui.remove(node);
        ui.try_prepare_frame().unwrap();
    }
}

#[cfg(test)]
mod editor_layout_cache_tests {
    use super::*;
    use crate::text_layout::FontFamily;

    #[test]
    fn cache_charges_named_font_key_and_bounds_display_text() {
        let mut ui = Ui::new(300., 200.);
        let editor = ui.text_input(ui.root(), "Editor", ui.signal("short".into()), 100., true);
        ui.prepare_frame();
        ui.input.focus(&ui.scene, Some(editor.node));
        let storage = Rc::downgrade(&ui.storage);
        let scene = ui.scene.borrow();
        let font = FontStyle::default();
        let first = editor_layout(&storage, &scene, editor.node, "short", 12., None, &font);
        let second = editor_layout(&storage, &scene, editor.node, "short", 12., None, &font);
        assert!(Rc::ptr_eq(&first, &second));
        let huge_font = FontStyle {
            family: FontFamily::Named("f".repeat(EDITOR_CACHE_WEIGHT_LIMIT).into()),
            ..font.clone()
        };
        editor_layout(
            &storage,
            &scene,
            editor.node,
            "short",
            12.,
            None,
            &huge_font,
        );
        assert!(ui.storage.editor_layout.borrow().is_none());
        editor_layout(
            &storage,
            &scene,
            editor.node,
            &"x".repeat(EDITOR_CACHE_TEXT_LIMIT + 1),
            12.,
            None,
            &font,
        );
        assert!(ui.storage.editor_layout.borrow().is_none());
    }

    #[test]
    fn external_editor_handle_does_not_retain_ui_layout_snapshot() {
        let mut ui = Ui::new(300., 200.);
        let editor = ui.text_input(ui.root(), "Editor", ui.signal("short".into()), 100., true);
        ui.prepare_frame();
        ui.input.focus(&ui.scene, Some(editor.node));
        let layout = Rc::downgrade(&ui.storage.editor_layout.borrow().as_ref().unwrap().layout);
        drop(ui);
        assert!(layout.upgrade().is_none());
        // An externally retained handle may refresh its surviving scene, but
        // cannot resurrect ownership of the destroyed document's cache.
        editor.refresh();
        assert!(layout.upgrade().is_none());
    }
}

#[cfg(test)]
mod disabled_reentry_tests {
    use super::*;
    #[test]
    fn disabling_can_remove_the_target_during_cleanup_without_resurrecting_metadata() {
        for pointer in [false, true] {
            let mut ui = Ui::new(100., 100.);
            let node = ui.container(
                ui.root(),
                Layout::Overlay,
                Style {
                    width: Some(40.),
                    height: Some(40.),
                    ..Default::default()
                },
            );
            let weak = ui.downgrade();
            ui.on_event(node, true, move |cx| {
                if matches!(cx.event, InputEvent::PointerDown { .. }) {
                    cx.capture_pointer();
                }
                if (pointer && matches!(cx.event, InputEvent::PointerCancel))
                    || (!pointer && matches!(cx.event, InputEvent::Blur))
                {
                    weak.upgrade().unwrap().remove(node);
                }
            });
            ui.prepare_frame();
            ui.input.focus(&ui.scene, Some(node));
            if pointer {
                ui.dispatch(InputEvent::PointerDown {
                    x: 10.,
                    y: 10.,
                    button: PointerButton::Primary,
                });
            }
            ui.set_disabled(node, true);
            assert!(!ui.scene.borrow().contains(node));
            assert!(ui.input.options(node).is_none());
            assert!(ui.semantics.borrow().get(node).is_none());
            assert_eq!(ui.input.focused(), None);
            assert_eq!(ui.input.captured(), None);
        }
    }
}

#[cfg(test)]
mod mount_initialization_tests {
    use super::*;
    #[test]
    fn successful_binding_does_not_retain_initialization_token() {
        let mut ui = Ui::new(100., 100.);
        let owner = ui.label(ui.root(), "owner", Style::default());
        let token = ui.begin_mount_initialization();
        token.set_root(owner);
        let weak = Rc::downgrade(&token);
        {
            let _scope = token.enter();
            ui.bind(owner, || {});
        }
        drop(token);
        assert!(weak.upgrade().is_none());
        assert_eq!(ui.runtime.effect_count(), 1);
    }
}
