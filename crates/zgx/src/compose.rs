//! Declarative, retained components and children. Construction runs once per mount;
//! reactive properties update individual nodes without rebuilding their component.
use crate::{
    compose_style::{Typography, Variants, mount_style_with_intrinsic},
    input::{EventContext, EventPhase, InputEvent, Navigation, PointerButton},
    reactive::{Runtime, ServiceScope, Signal},
    scene::{Layout, NodeId, NodeKind},
    semantics::{Live, Role, SemanticNode},
    style::{Styled, Styles},
    theme::tok,
    widgets::{Ui, WeakUi},
};
use std::{
    any::Any,
    cell::{Cell, RefCell},
    collections::HashMap,
    future::Future,
    pin::Pin,
    rc::{Rc, Weak},
};

#[path = "compose_animation.rs"]
mod animation;
#[path = "compose_variable.rs"]
mod variable;
pub use variable::{VariableHeights, measured_rows, measured_virtual_list, variable_virtual_list};

/// A local UI future, supplied to a platform's wake-driven executor.
pub type LocalFuture = Pin<Box<dyn Future<Output = ()>>>;
trait Lease {
    fn finished(&self) -> bool;
}
struct CheckedLease<G, F> {
    guard: G,
    check: F,
}
impl<G, F: Fn(&G) -> bool> Lease for CheckedLease<G, F> {
    fn finished(&self) -> bool {
        (self.check)(&self.guard)
    }
}
/// Erases a cancel-on-drop executor guard without losing completion information.
pub struct TaskToken(Box<dyn Lease>);
impl TaskToken {
    /// A token holding `guard`, finished once `finished(&guard)` says so.
    /// Dropping the token drops the guard, which cancels its task.
    pub fn new<G: 'static>(guard: G, finished: impl Fn(&G) -> bool + 'static) -> Self {
        Self(Box::new(CheckedLease {
            guard,
            check: finished,
        }))
    }
}
/// Provider bridging components to a native or headless executor.
#[derive(Clone)]
pub struct TaskRunner(Rc<dyn Fn(LocalFuture) -> TaskToken>);
impl TaskRunner {
    /// Runs each future with `spawn`, which returns the token that cancels it.
    pub fn new(spawn: impl Fn(LocalFuture) -> TaskToken + 'static) -> Self {
        Self(Rc::new(spawn))
    }
    /// Runs futures on `executor`.
    pub fn from_executor(executor: Rc<RefCell<crate::task::LocalExecutor>>) -> Self {
        let spawner = executor.borrow().spawner();
        Self::new(move |future| {
            TaskToken::new(spawner.spawn_scoped(future), |task| {
                task.as_ref().is_none_or(|task| task.is_finished())
            })
        })
    }
    /// Run `future`; dropping the token cancels it.
    pub(crate) fn spawn(&self, future: LocalFuture) -> TaskToken {
        (self.0)(future)
    }
}
#[derive(Default)]
struct TaskOwner {
    tasks: RefCell<Vec<TaskToken>>,
    /// The length at which finished tasks are next dropped: twice what was
    /// left last time, so spawning stays constant time however many run
    /// (a scan per spawn made a burst of N spawns cost N²).
    prune_at: Cell<usize>,
}
/// Cloneable event-handler capability. It never keeps its component alive.
#[derive(Clone)]
pub struct Tasks {
    owner: Weak<TaskOwner>,
    runner: TaskRunner,
}
impl Tasks {
    /// Run `future` until it finishes or the component unmounts, which
    /// cancels it. Once the component has unmounted this does nothing.
    pub fn spawn(&self, future: impl Future<Output = ()> + 'static) {
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        if owner.tasks.borrow().len() >= owner.prune_at.get() {
            // Completion callbacks and guard destructors belong to the
            // executor. They can spawn another task through this same
            // capability; detach the current batch before invoking either,
            // preserving reentrant additions.
            let mut pending = std::mem::take(&mut *owner.tasks.borrow_mut());
            pending.retain(|task| !task.0.finished());
            owner.prune_at.set((pending.len() * 2).max(1));
            // Reuse the existing task vector on the ordinary non-reentrant path.
            let reentrant = owner.tasks.replace(pending);
            owner.tasks.borrow_mut().extend(reentrant);
        }
        let task = (self.runner.0)(Box::pin(future));
        owner.tasks.borrow_mut().push(task);
    }
}
type Registry = Rc<RefCell<HashMap<String, Vec<NodeId>>>>;
/// What a virtual list knows of one of its rows: the run that last showed
/// it, and the index and width it was last placed at.
struct Placed {
    run: u64,
    at: Option<(usize, u32)>,
}
#[derive(Clone)]
pub(crate) struct Environment {
    pub(crate) services: ServiceScope,
    typography: Signal<Typography>,
    registry: Registry,
    portal: Option<(NodeId, Signal<bool>)>,
    menu: Option<Rc<crate::compose_menu::MenuContext>>,
    measure: crate::rich_text::TextMeasure,
    /// The theme scope styles in this subtree resolve tokens from.
    pub(crate) theme: Rc<crate::theme::ThemeScope>,
}
/// Mount-time component context. State is allocated once; there are no positional hooks.
pub struct Context {
    runtime: Runtime,
    environment: Environment,
    resources: Vec<Box<dyn Any>>,
    tasks: Option<Rc<TaskOwner>>,
}
impl Context {
    fn new(runtime: Runtime, environment: Environment) -> Self {
        Self {
            runtime,
            environment,
            resources: Vec::new(),
            tasks: None,
        }
    }
    /// The reactive runtime the component's signals live in.
    pub fn runtime(&self) -> Runtime {
        self.runtime.clone()
    }
    /// A signal holding `initial`, made once as the component mounts.
    pub fn state<T: 'static>(&self, initial: T) -> Signal<T> {
        self.runtime.signal(initial)
    }
    /// The nearest `T` an ancestor provides (see [`provide`]), if any.
    pub fn try_service<T: 'static>(&self) -> Option<Rc<T>> {
        self.environment.services.get()
    }
    /// The nearest `T` an ancestor provides (see [`provide`]). Panics if none
    /// does.
    pub fn service<T: 'static>(&self) -> Rc<T> {
        self.try_service().unwrap_or_else(|| {
            panic!(
                "missing component provider for {}",
                std::any::type_name::<T>()
            )
        })
    }
    /// Keep `resource` alive until the component unmounts.
    pub fn retain(&mut self, resource: impl Any) {
        self.resources.push(Box::new(resource));
    }
    /// Spawns futures that are cancelled when the component unmounts. Panics
    /// without a [`TaskRunner`] provider; zgx-desktop's windows provide one.
    pub fn tasks(&mut self) -> Tasks {
        let runner = self.service::<TaskRunner>();
        let owner = self
            .tasks
            .get_or_insert_with(|| Rc::new(TaskOwner::default()));
        Tasks {
            owner: Rc::downgrade(owner),
            runner: (*runner).clone(),
        }
    }
    /// Measures rich text as this tree's layout does.
    pub fn text_measure(&self) -> crate::rich_text::TextMeasure {
        self.environment.measure.clone()
    }
    /// The window's display-paced frame clock. Await it from [`Context::tasks`]
    /// to animate once per refresh; see [`crate::frame`].
    pub fn frames(&self) -> crate::frame::FrameClock {
        (*self.service::<crate::frame::FrameClock>()).clone()
    }
    /// The theme scope this component mounts in: read tokens from it (in an
    /// effect, to follow theme changes), or make a child scope.
    pub fn theme(&self) -> Rc<crate::theme::ThemeScope> {
        self.environment.theme.clone()
    }
    /// Captures the caller's providers. The receiver owns the mounted content;
    /// visual styles inherit at the insertion point, while lexical captures stay intact.
    pub fn slot(&self, build: impl FnOnce(&mut Context) -> View + 'static) -> Slot {
        Slot {
            build: Box::new(build),
            services: self.environment.services.clone(),
        }
    }
    fn finish(mut self, ui: &mut Ui, root: NodeId) {
        if let Some(tasks) = self.tasks.take() {
            self.resources.push(Box::new(tasks));
        }
        if !self.resources.is_empty() {
            ui.retain(root, self.resources);
        }
    }
}
/// A single-use child slot with lexical providers and ordinary Rust captures.
pub struct Slot {
    build: Box<dyn FnOnce(&mut Context) -> View>,
    services: ServiceScope,
}
impl From<Slot> for View {
    fn from(slot: Slot) -> Self {
        View::new(Kind::Slot(slot))
    }
}
type Builder = Box<dyn FnOnce(&mut Context) -> View>;
type RegionBuilder = Box<dyn FnOnce(&mut Ui, NodeId, Environment)>;
type VirtualBuilder = Box<dyn FnOnce(&mut Ui, NodeId, Environment, ListOptions)>;
/// What a virtual list is mounted with, beside its rows: its scrollbar,
/// keyboard navigation, and what it reports of its edges.
pub(crate) struct ListOptions {
    pub(crate) scrollbar: bool,
    pub(crate) keyboard_navigation: bool,
    /// `View::overscroll_contain`.
    pub(crate) contain: bool,
    pub(crate) edges: Option<Box<ListEdges>>,
}
/// What a virtual list reports of its edges: its end and its start coming
/// in reach (`View::on_end`, `View::on_start`) and whether it rests at its
/// end (`View::on_at_end`). Boxed: few lists have any.
#[derive(Default)]
pub(crate) struct ListEdges {
    end: Option<EdgeWatch>,
    start: Option<EdgeWatch>,
    at_end: Option<Box<dyn FnMut(bool)>>,
}
impl ListEdges {
    /// Whether its start is watched (`View::on_start`): rows coming in
    /// above are then kept from moving the rows in view.
    pub(crate) fn watches_start(&self) -> bool {
        self.start.is_some()
    }
}
/// What a virtual list calls as an edge comes within `distance` points of
/// its viewport's.
pub(crate) struct EdgeWatch {
    distance: f32,
    reached: Box<dyn FnMut()>,
}
/// How close to its end a list rests at it (`View::on_at_end`), in points:
/// what a fractional scroll position leaves.
const AT_END: f32 = 4.;
/// A virtual list's region: its mount, given its options. Out of line: a
/// list's mount is rare beside the elements `open_element` makes, which it
/// is inlined into.
#[inline(never)]
fn virtual_region(
    build: VirtualBuilder,
    scrollbar: bool,
    keyboard_navigation: bool,
    contain: bool,
    edges: Option<Box<ListEdges>>,
) -> RegionBuilder {
    let options = ListOptions {
        scrollbar,
        keyboard_navigation,
        contain,
        edges,
    };
    Box::new(move |ui: &mut Ui, root, environment| build(ui, root, environment, options))
}
/// Report a virtual list's edges (its content `extent`, scrolled by
/// `offset`, in a viewport `size`): call `edges.end.reached` once each time
/// its end comes within `edges.end.distance`, and `edges.start.reached`
/// its start, again only once its content has changed size, so a list
/// shorter than its viewport asks until it fills or nothing comes; and
/// tell `edges.at_end` whether it rests at its end, as that changes. Rust
/// watches; the script runs only as an edge is reached or left.
pub(crate) fn watch_edges(
    ui: &mut Ui,
    root: NodeId,
    offset: Signal<f32>,
    size: Signal<(f32, f32)>,
    extent: Signal<f32>,
    edges: ListEdges,
) {
    let ListEdges {
        end,
        start,
        mut at_end,
    } = edges;
    let runtime = ui.runtime.clone();
    let weak = ui.downgrade();
    // An edge reached is told after the frame's layout, the list's
    // position settled: rows the call adds above then keep the rows in
    // view where they are.
    let tell = move |reached: &Rc<RefCell<Box<dyn FnMut()>>>| {
        let (reached, runtime) = (reached.clone(), runtime.clone());
        if let Some(ui) = weak.upgrade() {
            ui.after_layout(Box::new(move || {
                runtime.untracked(|| (reached.borrow_mut())());
            }));
        }
    };
    let shared = |watch: Option<EdgeWatch>| {
        watch.map(|watch| (watch.distance, Rc::new(RefCell::new(watch.reached))))
    };
    let (end, start) = (shared(end), shared(start));
    let runtime = ui.runtime.clone();
    let (mut end_at, mut start_at): (Option<u32>, Option<u32>) = (None, None);
    let mut was_at_end: Option<bool> = None;
    let effect = ui.runtime.effect(move || {
        let (offset, (_, height), extent) = (offset.get(), size.get(), extent.get());
        if height <= 0. {
            // Not laid out yet.
            return;
        }
        let to_end = extent - offset - height;
        if let Some(at_end) = &mut at_end {
            let now = to_end <= AT_END;
            if was_at_end.replace(now) != Some(now) {
                runtime.untracked(|| at_end(now));
            }
        }
        if let Some((distance, reached)) = &end
            && to_end <= *distance
            && end_at != Some(extent.to_bits())
        {
            end_at = Some(extent.to_bits());
            tell(reached);
        }
        if let Some((distance, reached)) = &start
            && offset <= *distance
            && start_at != Some(extent.to_bits())
        {
            start_at = Some(extent.to_bits());
            tell(reached);
        }
    });
    ui.retain(root, effect);
}
enum Kind {
    Container(Layout),
    Text(String),
    BoundText(Box<dyn FnMut() -> String>),
    RichText(Box<dyn FnMut() -> Vec<crate::compose_rich::TextSpan>>),
    Button,
    MenuItem(String),
    Submenu {
        label: String,
        open: Signal<bool>,
    },
    Menu {
        label: String,
        open: Signal<bool>,
        anchor: Box<View>,
    },
    Modal {
        label: String,
        open: Signal<bool>,
        anchor: Option<Box<View>>,
    },
    Scroll {
        offset: Signal<f32>,
        horizontal: bool,
    },
    Canvas(Box<dyn FnMut((f32, f32)) -> crate::canvas::Canvas>),
    Svg {
        label: String,
        source: Box<dyn FnMut() -> std::sync::Arc<crate::svg::SvgData>>,
    },
    #[cfg(target_os = "macos")]
    NativeSurface {
        label: String,
        source: Box<dyn FnMut() -> std::rc::Rc<crate::native_surface::NativeSurface>>,
    },
    Image {
        label: String,
        source: Box<dyn FnMut() -> std::sync::Arc<crate::image::ImageData>>,
    },
    Progress {
        label: String,
        value: Signal<f32>,
    },
    Checkbox {
        label: String,
        value: Signal<bool>,
        /// `View::mixed`: neither on nor off (some of a group are).
        mixed: Option<Signal<bool>>,
    },
    Slider {
        label: String,
        value: Signal<f32>,
        range: std::ops::RangeInclusive<f32>,
    },
    Editor {
        label: String,
        value: crate::widgets::EditorValue,
        multiline: bool,
    },
    Component(Builder),
    Provider(Box<dyn FnOnce(&mut Context)>, Box<View>),
    Slot(Slot),
    Region(RegionBuilder),
    VirtualList(VirtualBuilder),
}
type EventListener = Box<dyn FnMut(&mut EventContext)>;

/// A mount description, consumed into owned retained nodes and subscriptions.
/// `.child`/`.children` compose views; `.style` and fluent methods apply to their root.
/// A view's overlay options (`View::placement`, `modal_placement`,
/// `trigger`).
#[derive(Default)]
struct PortalOptions {
    placement: Option<crate::compose_popover::Placement>,
    /// `layer`: a non-modal layer (toasts).
    layer: bool,
    /// `View::autofocus`.
    autofocus: Option<bool>,
    modal_placement: Option<crate::compose_modal::ModalPlacement>,
    trigger: Option<View>,
    /// `View::lazy`: built the first time it opens.
    lazy: bool,
    /// `View::open_over`: the id of the item over the trigger.
    over: Option<Rc<dyn Fn() -> String>>,
    /// `View::arrow`: how far a popover's arrow reaches from its edge.
    arrow: Option<f32>,
    /// `View::at_caret`: placed at the caret of the editor in its trigger,
    /// focus left there.
    at_caret: bool,
}
/// A declarative view: a node to mount, its styles, children, handlers and
/// semantics. Built with the functions here ([`column()`], [`text()`],
/// [`button()`]…) and the builder methods, then mounted with `Ui::mount`.
pub struct View {
    kind: Kind,
    styles: Styles,
    children: Vec<View>,
    variants: Variants,
    reactive_style: Option<crate::compose_style::DynamicStyle>,
    disabled: Option<Box<dyn FnMut() -> bool>>,
    click: Option<Box<dyn FnMut(crate::input::Modifiers)>>,
    events: Vec<EventListener>,
    focusable: Option<bool>,
    id: Option<String>,
    contents: bool,
    mounted: Vec<Box<dyn FnOnce(NodeHandle)>>,
    /// What few views set, boxed: views move by value through every
    /// builder call and mount level, and most carry none of it.
    rare: Option<Box<Rare>>,
}
/// Ranges of a text marked with a colour (`View::decorations`).
type Decorations = Vec<(std::ops::Range<usize>, crate::scene::Color)>;
/// Ranges of a text underlined (`View::underlines`).
type Underlines = Vec<crate::widgets::EditorUnderline>;
/// What a code editor adds to a text area: its lines' colours, ranges
/// marked behind its text, its current line and what Tab inserts.
#[derive(Default)]
pub(crate) struct CodeOptions {
    highlighter: Option<crate::widgets::SpanHighlighter>,
    decorations: Option<Box<dyn FnMut() -> Decorations>>,
    underlines: Option<Box<dyn FnMut() -> Underlines>>,
    current_line: Option<Box<dyn FnMut() -> Option<crate::scene::Color>>>,
    indent: Option<String>,
    on_caret: Option<Box<dyn FnMut(crate::widgets::Caret)>>,
    on_top_line: Option<Box<dyn FnMut(usize)>>,
}
/// `View`'s fields few views set (`View::rare`).
#[derive(Default)]
pub(crate) struct Rare {
    read_only: Option<Box<dyn FnMut() -> bool>>,
    keymap: crate::actions::Keymap,
    pub(crate) drag_preview: Option<crate::compose_drag::PreviewBuilder>,
    pub(crate) context_menu: Option<crate::compose_context_menu::ContextMenuBuilder>,
    /// Boxed: a `Styles` is 432 bytes, and every view carries this field
    /// through each builder call.
    pub(crate) context_menu_style: Option<Box<Styles>>,
    pub(crate) context_menu_below: Option<bool>,
    /// Whether the pointer stops at the view (`pointer_events`), followed.
    pointer_events: Option<Box<dyn FnMut() -> bool>>,
    scrollbar: Option<bool>,
    follow_end: Option<bool>,
    /// `View::overscroll_contain`: a wheel over it stays with it.
    overscroll_contain: bool,
    animation: Option<animation::Animated>,
    keyboard_navigation: Option<bool>,
    /// `View::on_end`, `on_start`, `at_end`: a virtual list's edges.
    edges: Option<Box<ListEdges>>,
    /// `View::active_descendant`: the id of the item it is on.
    active_descendant: Option<Box<dyn Fn() -> Option<String>>>,
    dismiss_backdrop: Option<bool>,
    /// Where an overlay opens and a submenu's own trigger row: boxed, as
    /// few views have them (views are moved through the mount recursion).
    portal: Option<Box<PortalOptions>>,
    layout_target: Option<Signal<Option<NodeId>>>,
    trigger_id: Option<String>,
    menu_trigger: Option<Signal<bool>>,
    /// Whether a menu item is checked, followed, and whether it is a radio.
    menu_check: Option<(Rc<dyn Fn() -> bool>, bool)>,
    /// The shortcut a menu item shows at its end.
    shortcut: Option<String>,
    /// A menu item that leaves its menu open (a toggle, a pin).
    menu_keep_open: bool,
    /// A slider's step (`View::step`).
    slider_step: Option<f64>,
    /// A popover at least as wide as its trigger (`match_anchor_width`).
    match_anchor_width: bool,
    /// An overlay's panel left out of the accessibility tree (a tooltip's,
    /// which describes its view instead).
    panel_hidden: bool,
    accessible_label: Option<String>,
    accessibility: Option<Box<Accessibility>>,
    placeholder: Option<Box<dyn FnMut() -> String>>,
    placeholder_color: Option<Box<dyn FnMut() -> crate::scene::Color>>,
    /// The caret's colour (followed) and width (`View::caret_color`).
    caret_color: Option<Box<dyn FnMut() -> Option<crate::scene::Color>>>,
    caret_width: Option<f32>,
    select_on_focus: Option<bool>,
    secure: Option<bool>,
    input_mode: Option<crate::text_edit::InputMode>,
    spellcheck: Option<Box<dyn FnMut() -> bool>>,
    line_numbers: Option<bool>,
    /// What a code editor adds to a text area (`View::highlighter`, …).
    code: Option<Box<CodeOptions>>,
    /// `WINDOW_DRAG` (`View::window_drag`) and `HOLLOW` (`View::hollow`):
    /// rare, tested together as the element mounts.
    mount_flags: u8,
    /// `View::selectable`.
    selectable: Option<bool>,
    /// `View::selection_color`, `selected_text_color`, `selection_radius`:
    /// few views have them.
    selection_style: Option<Box<crate::selection::SelectionStyle>>,
    /// `View::row_text`.
    row_text: Option<crate::selection::RowText>,
    /// `View::canvas_view`: a canvas's pan and zoom, followed.
    canvas_view: Option<Box<dyn FnMut() -> crate::scene::CanvasView>>,
    /// `View::grid_fill`: a grid over the box, followed.
    grid_fill: Option<Box<dyn FnMut() -> Option<crate::scene::GridFill>>>,
    /// Conditional children (`child_when`), mounted after the others.
    conditional: Vec<RegionBuilder>,
}
impl Rare {
    /// `View::refine`'s part: what an outer view sets over its inner one.
    fn merge_into(self, inner: &mut Rare) {
        if self.trigger_id.is_some() {
            inner.trigger_id = self.trigger_id;
        }
        if self.dismiss_backdrop.is_some() {
            inner.dismiss_backdrop = self.dismiss_backdrop;
        }
        if self.portal.is_some() {
            inner.portal = self.portal;
        }
        if self.edges.is_some() {
            inner.edges = self.edges;
        }
        if self.active_descendant.is_some() {
            inner.active_descendant = self.active_descendant;
        }
        if self.keyboard_navigation.is_some() {
            inner.keyboard_navigation = self.keyboard_navigation;
        }
        if self.scrollbar.is_some() {
            inner.scrollbar = self.scrollbar;
        }
        if self.follow_end.is_some() {
            inner.follow_end = self.follow_end;
        }
        if self.overscroll_contain {
            inner.overscroll_contain = true;
        }
        if self.drag_preview.is_some() {
            inner.drag_preview = self.drag_preview;
        }
        if self.context_menu.is_some() {
            inner.context_menu = self.context_menu;
        }
        if self.context_menu_style.is_some() {
            inner.context_menu_style = self.context_menu_style;
        }
        if self.context_menu_below.is_some() {
            inner.context_menu_below = self.context_menu_below;
        }
        if self.pointer_events.is_some() {
            inner.pointer_events = self.pointer_events;
        }
        if self.read_only.is_some() {
            inner.read_only = self.read_only;
        }
        if self.placeholder.is_some() {
            inner.placeholder = self.placeholder;
        }
        if self.canvas_view.is_some() {
            inner.canvas_view = self.canvas_view;
        }
        if self.grid_fill.is_some() {
            inner.grid_fill = self.grid_fill;
        }
        if self.placeholder_color.is_some() {
            inner.placeholder_color = self.placeholder_color;
        }
        if self.caret_color.is_some() {
            inner.caret_color = self.caret_color;
        }
        if self.caret_width.is_some() {
            inner.caret_width = self.caret_width;
        }
        if self.select_on_focus.is_some() {
            inner.select_on_focus = self.select_on_focus;
        }
        if self.spellcheck.is_some() {
            inner.spellcheck = self.spellcheck;
        }
        if self.secure.is_some() {
            inner.secure = self.secure;
        }
        if self.input_mode.is_some() {
            inner.input_mode = self.input_mode;
        }
        if self.line_numbers.is_some() {
            inner.line_numbers = self.line_numbers;
        }
        if self.code.is_some() {
            inner.code = self.code;
        }
        if self.selectable.is_some() {
            inner.selectable = self.selectable;
        }
        if self.selection_style.is_some() {
            inner.selection_style = self.selection_style;
        }
        if self.row_text.is_some() {
            inner.row_text = self.row_text;
        }
        inner.keymap.extend(self.keymap);
        inner.mount_flags |= self.mount_flags;
        inner.conditional.extend(self.conditional);
    }
}
impl View {
    /// The fields few views set, made on first use.
    pub(crate) fn rare_mut(&mut self) -> &mut Rare {
        self.rare.get_or_insert_default()
    }
    /// What a slider moves by: an arrow key moves one step (Page Up and
    /// Down, or Shift with an arrow, ten), and dragging lands on steps from
    /// its start. Without one, an arrow moves a hundredth of its range.
    pub fn step(mut self, step: f32) -> Self {
        assert!(
            matches!(self.kind, Kind::Slider { .. }),
            "step requires a slider"
        );
        assert!(step.is_finite() && step > 0., "a slider's step is positive");
        self.rare_mut().slider_step = Some(step as f64);
        self
    }
    /// A popover or menu at least as wide as its trigger, as a select's
    /// list is (Radix's `--radix-popper-anchor-width`).
    pub fn match_anchor_width(mut self) -> Self {
        self.rare_mut().match_anchor_width = true;
        self
    }
    /// Leave an overlay's panel out of the accessibility tree.
    pub(crate) fn panel_hidden(mut self) -> Self {
        self.rare_mut().panel_hidden = true;
        self
    }
    /// Merge `styles` over this view's own (the crate's builders use it).
    pub(crate) fn merge_styles(mut self, styles: &Styles) -> Self {
        self.styles.merge(styles);
        self
    }
    /// Set the controlled checked state of a menu item, including accessibility.
    pub fn menu_checked(self, checked: bool) -> Self {
        self.menu_checked_when(move || checked)
    }
    /// A menu item checked while `checked` says so, followed: a check in
    /// the item's leading gutter, and a checkbox item to assistive
    /// technology (`menu_radio`: a radio item, a dot).
    pub fn menu_checked_when(mut self, checked: impl Fn() -> bool + 'static) -> Self {
        assert!(
            matches!(self.kind, Kind::MenuItem(_)),
            "menu_checked requires a menu item"
        );
        let radio = self
            .rare
            .as_ref()
            .and_then(|r| r.menu_check.as_ref())
            .is_some_and(|c| c.1);
        self.rare_mut().menu_check = Some((Rc::new(checked), radio));
        self
    }
    /// A checked menu item that is one of a group, chosen alone: a dot
    /// where a check would be (`menu_checked_when` says which).
    pub fn menu_radio(mut self) -> Self {
        assert!(
            matches!(self.kind, Kind::MenuItem(_)),
            "menu_radio requires a menu item"
        );
        let rare = self.rare_mut();
        match &mut rare.menu_check {
            Some(check) => check.1 = true,
            None => rare.menu_check = Some((Rc::new(|| false), true)),
        }
        self
    }
    /// The keys that do what a menu item does, shown at its end in the
    /// tertiary text colour and told to assistive technology ("⌘C").
    pub fn shortcut(mut self, keys: impl Into<String>) -> Self {
        self.rare_mut().shortcut = Some(keys.into());
        self
    }
    /// A menu item that leaves its menu open when chosen (a toggle, a pin):
    /// the click still runs, keyboard and pointer alike.
    pub fn keep_open(mut self) -> Self {
        assert!(
            matches!(self.kind, Kind::MenuItem(_)),
            "keep_open requires a menu item"
        );
        self.rare_mut().menu_keep_open = true;
        self
    }
    /// Transform decoded image/SVG painting around its allocated center without
    /// changing layout. Use `image_signal` and `ImageData::transformed` for a
    /// reactive matrix. This method requires an image view.
    pub fn with_transformation(mut self, transformation: crate::affine::Affine) -> Self {
        if let Some(animated) = self.rare.as_mut().and_then(|r| r.animation.as_mut()) {
            let frames = animated
                .animation
                .frames()
                .iter()
                .map(|frame| crate::animation::Frame {
                    image: std::sync::Arc::new(frame.image.transformed(transformation)),
                    duration: frame.duration,
                })
                .collect();
            animated.animation = std::sync::Arc::new(
                crate::animation::Animation::new(frames, animated.animation.loop_count())
                    .expect("transforms preserve animation limits"),
            );
        }
        self.kind = match self.kind {
            Kind::Image { label, mut source } => Kind::Image {
                label,
                source: Box::new(move || std::sync::Arc::new(source().transformed(transformation))),
            },
            Kind::Svg { label, mut source } => Kind::Svg {
                label,
                source: Box::new(move || std::sync::Arc::new(source().transformed(transformation))),
            },
            _ => panic!("with_transformation requires an image or SVG view"),
        };
        self
    }
    /// Draw the image dithered in dots `dot` points square, as Zeron dithers
    /// its wallpapers (see `ImageData::dithered`): worked out on the GPU as
    /// the image draws, following it as it moves, scales and covers. This
    /// method requires an image view.
    pub fn dither(mut self, dot: f32) -> Self {
        self.kind = match self.kind {
            Kind::Image { label, mut source } => Kind::Image {
                label,
                source: Box::new(move || std::sync::Arc::new(source().dithered(dot))),
            },
            _ => panic!("dither requires an image view"),
        };
        self
    }
    /// Draw the image as characters over itself, revealed by its shader
    /// parameters (see `ImageData::characters`). This method requires an
    /// image view.
    pub fn characters(mut self, pixel: f32) -> Self {
        self.kind = match self.kind {
            Kind::Image { label, mut source } => Kind::Image {
                label,
                source: Box::new(move || std::sync::Arc::new(source().characters(pixel))),
            },
            _ => panic!("characters requires an image view"),
        };
        self
    }
    /// Paint the SVG in `color` instead of its own colours. This method
    /// requires an SVG view.
    pub fn svg_tint(mut self, color: crate::scene::Color) -> Self {
        self.kind = match self.kind {
            Kind::Svg { label, mut source } => Kind::Svg {
                label,
                source: Box::new(move || std::sync::Arc::new(source().tinted(color))),
            },
            _ => panic!("svg_tint requires an SVG view"),
        };
        self
    }
    fn new(kind: Kind) -> Self {
        Self {
            kind,
            styles: Styles::new(),
            children: Vec::new(),
            variants: Variants::default(),
            reactive_style: None,
            disabled: None,
            click: None,
            events: Vec::new(),
            focusable: None,
            id: None,
            contents: false,
            mounted: Vec::new(),

            rare: None,
        }
    }
    /// Lay this container out as its last child would in its place, as CSS
    /// `display: contents` does: the child's size, flex and position
    /// properties (percentages and `absolute` insets against this view's
    /// parent) are this view's, and it fills this view's box. Of this
    /// view's own styles only margins (added to the child's), paint and
    /// clipping apply. Children before the last are placed over it at its
    /// origin. For a region that shows one view at a time (`switch`) and
    /// should not be a layout box of its own; see `Scene::set_contents`.
    pub fn contents(mut self) -> Self {
        self.contents = true;
        self
    }
    /// A checkbox's mixed state, while `mixed` is true: a dash in place of
    /// the checkmark, and "mixed" to assistive technology, as a "select
    /// all" box shows some of its group on. Only checkboxes take it.
    pub fn mixed(mut self, mixed: Signal<bool>) -> Self {
        match &mut self.kind {
            Kind::Checkbox { mixed: slot, .. } => *slot = Some(mixed),
            _ => panic!("mixed is only supported on checkboxes"),
        }
        self
    }
    /// Show an interactive overlay scrollbar on a scroll viewport or virtual list.
    pub fn scrollbar(mut self, visible: bool) -> Self {
        self.rare_mut().scrollbar = Some(visible);
        self
    }
    /// Keep a wheel or trackpad gesture that starts over this scroll view,
    /// virtual list or text area with it at its edges, as CSS's
    /// `overscroll-behavior: contain`: what it can't scroll goes to none of
    /// the scroll views around it (no page moving under a transcript, nor
    /// stretching), its momentum included. Without it a gesture at an edge
    /// goes on to them.
    pub fn overscroll_contain(mut self) -> Self {
        self.rare_mut().overscroll_contain = true;
        self
    }
    /// Keep a scroll viewport at its end while its content grows, as long as
    /// it was at the end (a chat transcript); scrolling away releases it.
    pub fn follow_end(mut self, follow: bool) -> Self {
        self.rare_mut().follow_end = Some(follow);
        self
    }
    /// Call `reached` as a virtual list's end comes within `distance` points
    /// of the bottom of its viewport (an infinite scroll's next page): once
    /// each time, and again once its content has grown. Its content shorter
    /// than its viewport is in reach too, so it asks until it fills or a
    /// call adds nothing. Watched in Rust: nothing runs while it scrolls
    /// short of it. For a virtual list.
    pub fn on_end(mut self, distance: f32, reached: impl FnMut() + 'static) -> Self {
        self.edges_mut().end = Some(EdgeWatch {
            distance: distance.max(0.),
            reached: Box::new(reached),
        });
        self
    }
    /// Call `reached` as a virtual list's start comes within `distance`
    /// points of the top of its viewport (a history's earlier page): once
    /// each time, and again once its content has changed size. Rows the
    /// call adds above keep the rows in view where they are, a measured
    /// list's by key (`measured_virtual_list`, `measured_rows`). Watched in
    /// Rust, as `on_end` is. For a virtual list.
    pub fn on_start(mut self, distance: f32, reached: impl FnMut() + 'static) -> Self {
        self.edges_mut().start = Some(EdgeWatch {
            distance: distance.max(0.),
            reached: Box::new(reached),
        });
        self
    }
    /// Tell `changed` whether a virtual list rests at its end (within a
    /// few points of it), once laid out and then as that changes: a "jump
    /// to the latest" button shows while it does not. For a virtual list.
    pub fn on_at_end(mut self, changed: impl FnMut(bool) + 'static) -> Self {
        self.edges_mut().at_end = Some(Box::new(changed));
        self
    }
    fn edges_mut(&mut self) -> &mut ListEdges {
        self.rare_mut().edges.get_or_insert_default()
    }
    /// Enable bounded keyboard row navigation on a virtual list.
    pub fn keyboard_navigation(mut self, enabled: bool) -> Self {
        self.rare_mut().keyboard_navigation = Some(enabled);
        self
    }
    /// Configure whether clicking outside a modal or popover panel dismisses it.
    pub fn dismiss_on_backdrop(mut self, enabled: bool) -> Self {
        self.rare_mut().dismiss_backdrop = Some(enabled);
        self
    }
    /// Where an anchored popover or menu opens against its trigger (below
    /// it, start-aligned, 4 points away unless set).
    pub fn placement(mut self, placement: crate::compose_popover::Placement) -> Self {
        self.rare_mut().portal.get_or_insert_default().placement = Some(placement);
        self
    }
    /// Hear a person's paste into this editor before it goes in, from a
    /// key, a menu or a middle click, as HTML's `paste` event does: handed
    /// the clipboard's text (none where it holds no text, as with an
    /// image), and true takes the paste, so nothing goes in. For a composer
    /// that makes a pasted picture or a long text an attachment. It may
    /// read the clipboard. Only editor roots accept this.
    pub fn on_paste(self, hook: impl Fn(Option<&str>) -> bool + 'static) -> Self {
        self.on_mount(move |handle| {
            if let Some(ui) = handle.ui.upgrade() {
                ui.set_paste_hook(handle.node, Rc::new(hook));
            }
        })
    }
    /// The item this view is on while focus stays in it (`aria-activedescendant`):
    /// the id of a node, as `id()` names it, or `None`. An editor driving a
    /// list of suggestions (a mention's, `View::at_caret`) keeps focus and
    /// moves through them with its keys; assistive technology hears the
    /// one it is on. `id` is read again as what it reads changes.
    pub fn active_descendant(mut self, id: impl Fn() -> Option<String> + 'static) -> Self {
        self.rare_mut().active_descendant = Some(Box::new(id));
        self
    }
    /// A popover placed at the caret of the text editor in its trigger, not
    /// at the trigger's box, following it as it moves (typing, a click, a
    /// line wrapping, the editor scrolling, an input method's composition),
    /// as a mention's or a command's suggestions follow the caret. It takes
    /// no focus: keys go on to the editor, which drives it (Up, Down,
    /// Enter, Escape) with its own handlers, and tells assistive technology
    /// the suggestion it is on (`View::active_descendant`). Pointer presses
    /// outside it pass through, and close it only as the app says (`open`).
    pub fn at_caret(mut self) -> Self {
        self.rare_mut().portal.get_or_insert_default().at_caret = true;
        self
    }
    /// A non-modal layer over the window (toasts, a heads-up): a modal view
    /// made a layer takes no focus and has no backdrop, the pointer passes
    /// through it but for what it holds, which stays live while a dialog
    /// holds focus; it is raised over everything as it opens.
    pub fn layer(mut self) -> Self {
        self.rare_mut().portal.get_or_insert_default().layer = true;
        self
    }
    /// Open a menu with its item `id` over the trigger, as a pop-up button's
    /// menu (a select's) opens with the chosen item over the button, rather
    /// than below it. `id` is read as it opens and while open (the chosen
    /// item, from state); one not in the menu places it as usual.
    pub fn open_over(mut self, id: impl Fn() -> String + 'static) -> Self {
        self.rare_mut().portal.get_or_insert_default().over = Some(Rc::new(id));
        self
    }
    /// A popover's arrow, as AppKit's popovers have: it reaches `size`
    /// points from the panel's edge to its tip, on the side facing the
    /// trigger (flipping with it), pointing at the trigger's centre. Drawn
    /// with the panel's own background and border; the panel sits `size`
    /// further from the trigger to make room. Menus have none.
    pub fn arrow(mut self, size: f32) -> Self {
        self.rare_mut().portal.get_or_insert_default().arrow = (size > 0.).then_some(size);
        self
    }
    /// Build a modal's, popover's or menu's panel the first time it opens
    /// (and keep it), rather than as it mounts: a list's hundreds of row
    /// menus, tooltips and hover cards cost nothing (no nodes, styles or
    /// observers) until one opens. What is inside is not found (`find`) or
    /// in the accessibility tree before then.
    pub fn lazy(mut self) -> Self {
        self.rare_mut().portal.get_or_insert_default().lazy = true;
        self
    }
    /// Whether a modal, popover or menu focuses its first control as it
    /// opens (it does unless set false; then the panel holds focus).
    pub fn autofocus(mut self, autofocus: bool) -> Self {
        self.rare_mut().portal.get_or_insert_default().autofocus = Some(autofocus);
        self
    }
    /// A submenu's trigger, drawn by the app: a `menu_item` with content of
    /// its own (an icon, the label, a value, a chevron) instead of the
    /// label and the engine's chevron.
    pub fn trigger(mut self, trigger: View) -> Self {
        self.rare_mut().portal.get_or_insert_default().trigger = Some(trigger);
        self
    }
    /// Where a modal's panel sits in the window (centred unless set): a
    /// side sheet against an edge, a command menu near the top.
    pub fn modal_placement(mut self, placement: crate::compose_modal::ModalPlacement) -> Self {
        self.rare_mut()
            .portal
            .get_or_insert_default()
            .modal_placement = Some(placement);
        self
    }
    /// Assign an ID to a submenu's internally owned menu-item trigger.
    pub fn trigger_id(mut self, id: impl Into<String>) -> Self {
        self.rare_mut().trigger_id = Some(id.into());
        self
    }
    /// Add `child` after the existing children.
    pub fn child(mut self, child: impl Into<View>) -> Self {
        self.children.push(child.into());
        self
    }
    /// Add `children` after the existing children, in order.
    pub fn children(mut self, children: impl IntoIterator<Item = View>) -> Self {
        self.children.extend(children);
        self
    }
    /// A child that exists only while `select` returns `Some`: built from
    /// its value, mounted directly under this view after its other
    /// children, and removed when it returns `None`. Unlike `switch` it
    /// adds no region box, so while absent it costs no nodes (only the
    /// subscription to what `select` reads). A changed value rebuilds the
    /// child; an equal one keeps it. For overlays such as an error banner
    /// (an `absolute` child paints over its siblings and takes no room).
    pub fn child_when<K: PartialEq + Clone + 'static>(
        mut self,
        mut select: impl FnMut() -> Option<K> + 'static,
        mut build: impl FnMut(K, &mut Context) -> View + 'static,
    ) -> Self {
        self.rare_mut()
            .conditional
            .push(Box::new(move |ui: &mut Ui, parent, environment| {
                let weak = ui.downgrade();
                let runtime = ui.runtime.clone();
                let mut selected: Option<K> = None;
                let mut mounted: Option<NodeId> = None;
                ui.bind(parent, move || {
                    let key = select();
                    if selected == key {
                        return;
                    }
                    let Some(mut ui) = weak.upgrade() else {
                        return;
                    };
                    if !ui.scene.borrow().contains(parent) {
                        return;
                    }
                    runtime.untracked(|| {
                        let old = mounted.take();
                        if let Some(key) = key.clone() {
                            let mut cx = Context::new(runtime.clone(), environment.clone());
                            let view = build(key, &mut cx);
                            let mut transaction = MountTransaction::new(&ui, parent);
                            let node = mount(&mut ui, parent, view, environment.clone());
                            cx.finish(&mut ui, node);
                            transaction.commit();
                            mounted = Some(node);
                        }
                        if let Some(old) = old
                            && ui.scene.borrow().contains(old)
                        {
                            ui.remove(old);
                        }
                    });
                    selected = key;
                });
            }));
        self
    }
    /// Name the view, so tests and `ViewHandle::find` can reach its node.
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }
    /// Run `callback` when the view is clicked, or pressed from the keyboard
    /// once focused.
    pub fn on_click(self, mut callback: impl FnMut() + 'static) -> Self {
        self.on_click_with(move |_| callback())
    }
    /// `on_click`, told the modifier keys held as it was clicked (a
    /// ⌘-click opening in full, a ⇧-click selecting a range).
    pub fn on_click_with(
        mut self,
        callback: impl FnMut(crate::input::Modifiers) + 'static,
    ) -> Self {
        self.click = Some(Box::new(callback));
        self
    }
    /// Append an owned listener for capture, target and bubble events.
    /// Inspect `EventContext::phase` to select a phase. Listeners run in declaration
    /// order, before built-in handlers; component wrapper listeners follow inner ones.
    pub fn on_event(mut self, callback: impl FnMut(&mut EventContext) + 'static) -> Self {
        self.events.push(Box::new(callback));
        self
    }
    /// Add scoped bindings and literal key contexts to this retained component.
    pub fn keymap(mut self, keymap: crate::actions::Keymap) -> Self {
        self.rare_mut().keymap.extend(keymap);
        self
    }
    /// Handle a typed action at the target or during bubbling. The handler may
    /// stop propagation; leaving it unstopped allows an ancestor fallback.
    pub fn on_action<T: 'static>(
        self,
        mut callback: impl FnMut(&T, &mut EventContext) + 'static,
    ) -> Self {
        self.on_event(move |cx| {
            if cx.phase == EventPhase::Capture {
                return;
            }
            if let InputEvent::Action(action) = cx.event.clone()
                && let Some(value) = action.downcast_ref::<T>()
            {
                callback(value, cx);
            }
        })
    }
    /// CSS `pointer-events`: false lets the pointer through the view, and
    /// through descendants without listeners of their own, to what is under
    /// it (a full-window layer that holds a popover): presses, hover, the
    /// wheel, drops, and a native view's input under it.
    pub fn pointer_events(self, enabled: bool) -> Self {
        self.pointer_events_when(move || enabled)
    }
    /// `pointer_events`, followed: it changes as what `enabled` reads does.
    pub fn pointer_events_when(mut self, enabled: impl FnMut() -> bool + 'static) -> Self {
        self.rare_mut().pointer_events = Some(Box::new(enabled));
        self
    }
    /// A box laid out around content that may be put away: while nothing
    /// it holds is displayed (`hidden` or `invisible`), the pointer passes
    /// through it as `pointer_events(false)` lets it, to what is under it.
    /// A presence's full-size box holding a page kept mounted but hidden
    /// would otherwise take every press meant for what is under it.
    pub fn hollow(mut self) -> Self {
        self.rare_mut().mount_flags |= HOLLOW;
        self
    }
    /// Override whether this root accepts focus and participates in Tab traversal.
    /// Registering an event listener alone does not make a passive view focusable.
    pub fn focusable(mut self, focusable: bool) -> Self {
        self.focusable = Some(focusable);
        self
    }
    /// Styles merged over the view's own while the pointer is over it.
    pub fn hover(mut self, style: impl FnOnce(Styles) -> Styles) -> Self {
        self.variants.hover = Some(Box::new(style(Styles::new())));
        self
    }
    /// Styles merged over the view's own while it is pressed.
    pub fn active(mut self, style: impl FnOnce(Styles) -> Styles) -> Self {
        self.variants.active = Some(Box::new(style(Styles::new())));
        self
    }
    /// Styles merged over the view's own while it has keyboard focus.
    pub fn focus(mut self, style: impl FnOnce(Styles) -> Styles) -> Self {
        self.variants.focus = Some(Box::new(style(Styles::new())));
        self
    }
    /// Styles merged over the view's own while it is disabled.
    pub fn disabled_style(mut self, style: impl FnOnce(Styles) -> Styles) -> Self {
        self.variants.disabled = Some(Box::new(style(Styles::new())));
        self
    }
    /// Hover, active, focus and disabled styles that change. `styles` runs
    /// in the node's style effect, so it follows what it reads, and returns
    /// the four when they changed since its last call (`None` keeps them);
    /// they replace those `hover` and the others set. A view whose styles
    /// follow state (a tab, a component's variant) swaps them without
    /// mounting again.
    pub fn reactive_interaction(
        mut self,
        styles: impl FnMut() -> Option<[Option<Styles>; 4]> + 'static,
    ) -> Self {
        self.variants.reactive = Some(Box::new(styles));
        self
    }
    /// Disable the view: it takes no clicks or focus, shows its
    /// `disabled_style`, and assistive technology reports it disabled.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = Some(Box::new(move || disabled));
        self
    }
    /// `disabled`, followed: it changes as what `disabled` reads does.
    pub fn disabled_when(mut self, disabled: impl FnMut() -> bool + 'static) -> Self {
        self.disabled = Some(Box::new(disabled));
        self
    }
    /// Keep an editor selectable and focusable while preventing user edits.
    /// External model updates remain permitted. Only editor roots accept this.
    pub fn read_only(mut self, read_only: bool) -> Self {
        self.rare_mut().read_only = Some(Box::new(move || read_only));
        self
    }
    /// Reactively control an editor's read-only state without remounting it.
    pub fn read_only_when(mut self, read_only: impl FnMut() -> bool + 'static) -> Self {
        self.rare_mut().read_only = Some(Box::new(read_only));
        self
    }
    /// The name assistive technology reads for this node, instead of one
    /// made from its text: a switch, or a button showing only an icon. A
    /// node without a role of its own becomes a named group.
    pub fn accessible_label(mut self, label: impl Into<String>) -> Self {
        self.rare_mut().accessible_label = Some(label.into());
        self
    }
    /// `accessible_label` followed: a recycled row's name, made from what it
    /// shows now ("Unread, Grace Hopper, Lunch?, Oct 28"). Only while
    /// assistive technology listens (`Ui::set_assistive_technology`): with
    /// none, `label` is not called as what it reads changes.
    pub fn accessible_label_when(mut self, label: impl FnMut() -> String + 'static) -> Self {
        self.accessibility_mut().label = Some(Box::new(label));
        self
    }
    /// Named `label`, or as `followed` says where a component's caller
    /// gave a label that changes ([`FollowedLabel`]).
    pub fn accessible_label_or(
        self,
        label: impl Into<String>,
        followed: Option<&Followed>,
    ) -> Self {
        match followed {
            Some(followed) => {
                let followed = followed.clone();
                self.accessible_label_when(move || followed.get())
            }
            None => self.accessible_label(label),
        }
    }
    fn accessibility_mut(&mut self) -> &mut Accessibility {
        self.rare_mut()
            .accessibility
            .get_or_insert_with(Default::default)
    }
    /// What assistive technology takes this node for (ARIA's `role`),
    /// instead of what it is: a clickable row as a `ListBoxOption`, a row
    /// of buttons as a `Toolbar`. It keeps what it does.
    pub fn role(mut self, role: Role) -> Self {
        self.accessibility_mut().role = Some(role);
        self
    }
    /// A section title of `level` (1 for the top, as `h1`): screen readers
    /// list headings and move between them.
    pub fn heading(mut self, level: usize) -> Self {
        let accessibility = self.accessibility_mut();
        accessibility.role = Some(Role::Heading);
        accessibility.level = Some(level.max(1));
        self
    }
    /// Whether this option, tab or item is the chosen one, followed
    /// (`aria-selected`).
    pub fn selected_when(mut self, selected: impl FnMut() -> bool + 'static) -> Self {
        self.accessibility_mut().selected = Some(Box::new(selected));
        self
    }
    /// Whether this checkbox, switch or radio is on, followed
    /// (`aria-checked`). A checked radio is where Tab lands in its group.
    pub fn checked_when(mut self, checked: impl FnMut() -> bool + 'static) -> Self {
        self.accessibility_mut().checked = Some(Box::new(checked));
        self
    }
    /// A checkable menu item (`Role::MenuItem` with `checked_when`) that is
    /// one of a group: a radio item, `menuitemradio`, rather than
    /// `menuitemcheckbox`.
    pub fn radio_item(mut self) -> Self {
        self.accessibility_mut().radio_item = true;
        self
    }
    /// What this control's value reads as to assistive technology, followed:
    /// a select's chosen option, a combobox's text. Worked out only while
    /// assistive technology listens, as `accessible_label_when`.
    pub fn value_when(mut self, value: impl FnMut() -> String + 'static) -> Self {
        self.accessibility_mut().value = Some(Box::new(value));
        self
    }
    /// Whether what this control opens shows, followed (`aria-expanded`):
    /// a disclosure's content, a combobox's list, a tree item's children.
    pub fn expanded_when(mut self, expanded: impl FnMut() -> bool + 'static) -> Self {
        self.accessibility_mut().expanded = Some(Box::new(expanded));
        self
    }
    /// A number control's range and value, followed (`aria-valuemin`,
    /// `aria-valuemax`, `aria-valuenow`): a stepper, a spin button.
    pub fn numeric_when(
        mut self,
        min: f64,
        max: f64,
        value: impl FnMut() -> f64 + 'static,
    ) -> Self {
        self.accessibility_mut().numeric = Some((min, max, Box::new(value)));
        self
    }
    /// Announce changes to the text in this subtree (`aria-live`): a
    /// count, a "Sent" note, a message arriving.
    pub fn live(mut self, live: Live) -> Self {
        self.accessibility_mut().live = Some(live);
        self
    }
    /// Read after the name, as a hint (`aria-description`).
    pub fn accessible_description(mut self, description: impl Into<String>) -> Self {
        self.accessibility_mut().description = Some(description.into());
        self
    }
    /// What is wrong with this field's value, followed (`aria-invalid`
    /// and `aria-errormessage`): `None` while it is fine. Assistive
    /// technology hears the field is invalid, and why, as it reaches it.
    /// Worked out only while one listens, as `accessible_label_when`.
    pub fn error_when(mut self, error: impl FnMut() -> Option<String> + 'static) -> Self {
        self.accessibility_mut().error = Some(Box::new(error));
        self
    }
    /// Leave this node and its subtree out of the accessibility tree
    /// (`aria-hidden`): decoration, or what is said elsewhere already.
    pub fn accessible_hidden(mut self) -> Self {
        self.accessibility_mut().hidden = true;
        self
    }
    /// One Tab stop for the focusable views inside, the arrows along
    /// `axis` (and Home, End) moving among them, as ARIA's composite
    /// widgets behave: Tab lands on the one last focused, or the selected
    /// one (`selected_when`). A `ListBox` is vertical, a `Toolbar` or
    /// `TabList` horizontal, without asking.
    pub fn focus_group(mut self, axis: crate::input::FocusGroup) -> Self {
        self.accessibility_mut().focus_group = Some(axis);
        self
    }
    /// With nothing focused (and no modal open), keys go to this view and
    /// bubble from it, as a page's go to its body: where an app's shortcuts
    /// listen. The last mounted such view takes them.
    pub fn takes_unfocused_keys(self) -> Self {
        self.on_mount(|handle| {
            if let Some(ui) = handle.ui.upgrade() {
                ui.input.take_unfocused_keys(handle.node);
            }
        })
    }
    /// Go back or forward on the platform's gesture for it over this view:
    /// a trackpad's swipe between pages (macOS), where nothing under the
    /// fingers scrolls that way (`Ui::set_navigation_swipes`). The nearest
    /// such view around what is under the swipe takes it.
    pub fn on_navigate(self, mut callback: impl FnMut(Navigation) + 'static) -> Self {
        self.on_mount(|handle| {
            if let Some(ui) = handle.ui.upgrade() {
                ui.input.take_navigation(handle.node, &ui.scene.borrow());
            }
        })
        .on_action(move |navigation: &Navigation, cx| {
            callback(*navigation);
            cx.prevent_default();
            cx.stop_propagation();
        })
    }
    /// Hear every event the document is given while this view is mounted,
    /// before it goes where it goes, and one that goes nowhere (a key with
    /// nothing focused): `Ui::on_document_event`.
    pub fn on_document_event(self, listener: impl FnMut(&InputEvent) + 'static) -> Self {
        self.on_mount(move |handle| {
            if let Some(mut ui) = handle.ui.upgrade() {
                ui.on_document_event(handle.node, listener);
            }
        })
    }
    /// Run `f` with the mounted node once the view has mounted.
    pub fn on_mount(mut self, f: impl FnOnce(NodeHandle) + 'static) -> Self {
        self.mounted.push(Box::new(f));
        self
    }
    /// Styles merged over the view's own, computed in an effect: they change
    /// as what `style` reads does, without mounting again.
    pub fn reactive_style(mut self, mut style: impl FnMut() -> Styles + 'static) -> Self {
        self.reactive_style = Some(Box::new(move |out: &mut Option<Styles>| {
            *out = Some(style())
        }));
        self
    }
    /// Text an editor shows while it is empty, as HTML's `placeholder`: in
    /// the editor's typography, coloured the theme's `muted` (see
    /// `placeholder_color`). It is not part of the value, never changes the
    /// editor's size, and assistive technology reads it as the
    /// placeholder. Only editor roots accept this.
    pub fn placeholder(self, text: impl Into<String>) -> Self {
        let text = text.into();
        self.placeholder_when(move || text.clone())
    }
    /// A placeholder that follows the reactive values it reads.
    pub fn placeholder_when(mut self, text: impl FnMut() -> String + 'static) -> Self {
        self.rare_mut().placeholder = Some(Box::new(text));
        self
    }
    /// The placeholder's colour, instead of the theme's `muted`.
    pub fn placeholder_color(self, color: crate::scene::Color) -> Self {
        self.placeholder_color_when(move || color)
    }
    /// A placeholder colour that follows the reactive values it reads.
    pub fn placeholder_color_when(
        mut self,
        color: impl FnMut() -> crate::scene::Color + 'static,
    ) -> Self {
        self.rare_mut().placeholder_color = Some(Box::new(color));
        self
    }
    /// The caret's colour, instead of the text's. Only editor roots accept
    /// this.
    pub fn caret_color(self, color: crate::scene::Color) -> Self {
        self.caret_color_when(move || Some(color))
    }
    /// A caret colour that follows the reactive values it reads (`None`:
    /// the text's).
    pub fn caret_color_when(
        mut self,
        color: impl FnMut() -> Option<crate::scene::Color> + 'static,
    ) -> Self {
        self.rare_mut().caret_color = Some(Box::new(color));
        self
    }
    /// The caret's width in points, instead of the app's
    /// (`Ui::set_caret_width`). Only editor roots accept this.
    pub fn caret_width(mut self, points: f32) -> Self {
        self.rare_mut().caret_width = Some(points);
        self
    }
    /// Select an editor's whole text whenever it takes focus, as a browser's
    /// address bar does: the first click selects everything, the next places
    /// the caret (see `EditorHandle::set_select_on_focus`). Only editor
    /// roots accept this.
    pub fn select_on_focus(mut self, select: bool) -> Self {
        self.rare_mut().select_on_focus = Some(select);
        self
    }
    /// A password field: its text shows masked and can't be copied (see
    /// `EditorHandle::set_secure`). Only editor views take it.
    pub fn secure(mut self, secure: bool) -> Self {
        self.rare_mut().secure = Some(secure);
        self
    }
    /// What kind of text the editor takes, as HTML's `inputmode` says it
    /// (`InputMode::Email`, `Numeric`, …): which touch keyboard Windows
    /// shows and what a Wayland input method offers, none for
    /// `InputMode::None`. A hint, not a check: anything can still be typed.
    /// Only editor views take it.
    pub fn input_mode(mut self, mode: crate::text_edit::InputMode) -> Self {
        self.rare_mut().input_mode = Some(mode);
        self
    }
    /// Check the spelling as it is typed, as a browser's text fields do
    /// (`spellcheck`): the system's checker marks misspelled words with a
    /// dotted line, and the context menu offers its guesses, Learn Spelling
    /// and Ignore Spelling. Off by default; a password field never checks.
    /// Only editor views take it.
    pub fn spellcheck(self, check: bool) -> Self {
        self.spellcheck_when(move || check)
    }
    /// `spellcheck`, following the reactive values it reads (a setting):
    /// turned off, the marks go; turned on, the whole text is checked.
    pub fn spellcheck_when(mut self, check: impl FnMut() -> bool + 'static) -> Self {
        self.rare_mut().spellcheck = Some(Box::new(check));
        self
    }
    /// Number a multiline editor's lines in a gutter at its left, as a code
    /// editor does (see `EditorHandle::set_line_numbers`). Only editor roots
    /// accept this.
    pub fn line_numbers(mut self, numbered: bool) -> Self {
        self.rare_mut().line_numbers = Some(numbered);
        self
    }
    fn code_mut(&mut self) -> &mut CodeOptions {
        self.rare_mut().code.get_or_insert_with(Default::default)
    }
    /// Colour a text area's lines, as a code editor colours its syntax
    /// (see [`crate::widgets::Highlighter`]): asked for the lines in view,
    /// and again only for those that change. Only editor roots accept this.
    pub fn highlighter(
        mut self,
        highlighter: impl Fn(&str, u32) -> (crate::widgets::LineColors, u32) + 'static,
    ) -> Self {
        self.code_mut().highlighter = Some(crate::editor_lines::span_highlighter(
            std::rc::Rc::new(highlighter),
        ));
        self
    }
    /// Style a text area's lines, as `highlighter` colours them, with
    /// weights, italics, strikes and washes behind the text too (see
    /// [`crate::widgets::SpanStyle`]): a Markdown composer's bold, emphasis
    /// and inline code. Only editor roots accept this.
    pub fn span_highlighter(
        mut self,
        highlighter: impl Fn(&str, u32) -> (crate::widgets::LineSpans, u32) + 'static,
    ) -> Self {
        self.code_mut().highlighter = Some(std::rc::Rc::new(highlighter));
        self
    }
    /// Ranges of a text area's text (UTF-8 byte ranges) marked with a colour
    /// behind them, a search's matches; kept current as what `ranges` reads
    /// changes. Only those in view are drawn.
    pub fn decorations(
        mut self,
        ranges: impl FnMut() -> Vec<(std::ops::Range<usize>, crate::scene::Color)> + 'static,
    ) -> Self {
        self.code_mut().decorations = Some(Box::new(ranges));
        self
    }
    /// Ranges of a text area's text underlined (a spelling mistake's, a
    /// diagnostic's wave), kept current as what `ranges` reads changes. Only
    /// those in view are drawn.
    pub fn underlines(
        mut self,
        ranges: impl FnMut() -> Vec<crate::widgets::EditorUnderline> + 'static,
    ) -> Self {
        self.code_mut().underlines = Some(Box::new(ranges));
        self
    }
    /// A colour behind a text area's caret row (a code editor's current
    /// line), kept current as what `color` reads changes.
    pub fn current_line(
        mut self,
        color: impl FnMut() -> Option<crate::scene::Color> + 'static,
    ) -> Self {
        self.code_mut().current_line = Some(Box::new(color));
        self
    }
    /// What Tab inserts in a text area (`"    "`, `"\t"`) instead of moving
    /// focus: with lines selected it indents each, and Shift+Tab takes an
    /// indent off (see `EditorHandle::set_indent`).
    pub fn tab_indent(mut self, indent: impl Into<String>) -> Self {
        self.code_mut().indent = Some(indent.into());
        self
    }
    /// Call `told` with where a text area's caret is whenever it moves (a
    /// status bar's line and column); see `EditorHandle::on_caret`.
    pub fn on_caret(mut self, told: impl FnMut(crate::widgets::Caret) + 'static) -> Self {
        self.code_mut().on_caret = Some(Box::new(told));
        self
    }
    /// Call `told` with a text area's first line in view, from 1, whenever
    /// it changes; see `EditorHandle::on_top_line`.
    pub fn on_top_line(mut self, told: impl FnMut(usize) + 'static) -> Self {
        self.code_mut().on_top_line = Some(Box::new(told));
        self
    }
    /// Make the view a window drag region, like CSS's
    /// `-webkit-app-region: drag`: a primary press on it, or on anything
    /// inside it that does not take input itself, moves the window, and a
    /// double click zooms or minimizes it as the platform's setting says.
    /// Buttons, inputs and other focusable or pointer-capturing
    /// descendants keep their presses. The desktop host performs it (see
    /// `Ui::take_window_gesture`).
    pub fn window_drag(mut self) -> Self {
        self.rare_mut().mount_flags |= WINDOW_DRAG;
        self
    }
    /// Let the pointer select the text in this view and everything it holds,
    /// as CSS's `user-select: text` does: drag across any of its text, a
    /// double press selects a word and a triple press a paragraph, Shift
    /// extends, ⌘A / Ctrl+A selects all of it and ⌘C / Ctrl+C copies. One
    /// selection spans its text nodes in document order. `false` keeps the
    /// text of a part of a selectable view out (CSS `user-select: none`).
    /// Text is not selectable unless asked (markdown is). Editors select
    /// their own text, and links and buttons inside still click (a drag
    /// that starts on one selects).
    pub fn selectable(mut self, selectable: bool) -> Self {
        self.rare_mut().selectable = Some(selectable);
        self
    }
    /// The highlight behind this selectable region's selected text, in place
    /// of the theme's `selection` (and `selection.inactive`), as CSS's
    /// `::selection { background }` does. For a selectable view.
    pub fn selection_color(self, color: crate::scene::Color) -> Self {
        self.selection_color_when(move || color)
    }
    /// `selection_color`, following the reactive values it reads.
    pub fn selection_color_when(
        mut self,
        color: impl FnMut() -> crate::scene::Color + 'static,
    ) -> Self {
        self.rare_mut()
            .selection_style
            .get_or_insert_default()
            .background = Some(Box::new(color));
        self
    }
    /// The colour of this selectable region's selected text, in place of
    /// the theme's `selection.text`, as CSS's `::selection { color }` does;
    /// transparent keeps each glyph's own. Glyphs recolour as they are laid
    /// out: nothing is shaped again.
    pub fn selected_text_color(self, color: crate::scene::Color) -> Self {
        self.selected_text_color_when(move || color)
    }
    /// `selected_text_color`, following the reactive values it reads.
    pub fn selected_text_color_when(
        mut self,
        color: impl FnMut() -> crate::scene::Color + 'static,
    ) -> Self {
        self.rare_mut().selection_style.get_or_insert_default().text = Some(Box::new(color));
        self
    }
    /// The corner radius of this selectable region's highlight, in place of
    /// the theme's `selection.radius`, at most half a line; 0 draws its lines
    /// as plain rectangles. For a selectable view.
    pub fn selection_radius(self, radius: f32) -> Self {
        self.selection_radius_when(move || radius)
    }
    /// `selection_radius`, following the reactive values it reads.
    pub fn selection_radius_when(mut self, radius: impl FnMut() -> f32 + 'static) -> Self {
        self.rare_mut()
            .selection_style
            .get_or_insert_default()
            .radius = Some(Box::new(radius));
        self
    }
    /// A virtual list's plain text of row `index` (`None` past its end):
    /// copying a selection that spans rows the list has not mounted takes
    /// theirs from it, in order, so selecting all of a long transcript and
    /// copying it copies it all. Mounted rows copy as they show. Without
    /// it, only mounted rows copy. For a virtual list.
    pub fn row_text(mut self, text: impl Fn(usize) -> Option<String> + 'static) -> Self {
        self.rare_mut().row_text = Some(std::rc::Rc::new(text));
        self
    }
    /// A canvas's drawing placed by `view` (pan and zoom), following the
    /// reactive values it reads: a change repaints the drawing as it is,
    /// drawing nothing again (`Scene::set_canvas_view`). For a canvas.
    pub fn canvas_view(mut self, view: impl FnMut() -> crate::scene::CanvasView + 'static) -> Self {
        self.rare_mut().canvas_view = Some(Box::new(view));
        self
    }
    /// A grid painted over a canvas's box, under its drawing (a board's
    /// dots), following the reactive values `grid` reads: panned and zoomed
    /// by its fields, with no call per point (`Scene::set_grid_fill`). For
    /// a canvas.
    pub fn grid_fill(
        mut self,
        grid: impl FnMut() -> Option<crate::scene::GridFill> + 'static,
    ) -> Self {
        self.rare_mut().grid_fill = Some(Box::new(grid));
        self
    }
    // Wrapper properties refine the component's actual root without an extra layout box.
    fn refine(self, mut inner: View) -> View {
        inner.events.extend(self.events);
        if self.focusable.is_some() {
            inner.focusable = self.focusable;
        }
        inner.contents |= self.contents;
        inner.styles.merge(&self.styles);
        inner.children.extend(self.children);
        fn merge_variant(base: &mut Option<Box<Styles>>, patch: Option<Box<Styles>>) {
            if let Some(patch) = patch {
                match base {
                    Some(base) => base.merge(&patch),
                    None => *base = Some(patch),
                }
            }
        }
        merge_variant(&mut inner.variants.hover, self.variants.hover);
        merge_variant(&mut inner.variants.active, self.variants.active);
        merge_variant(&mut inner.variants.focus, self.variants.focus);
        merge_variant(&mut inner.variants.disabled, self.variants.disabled);
        if let Some(outer) = self.variants.reactive {
            inner.variants.reactive = Some(match inner.variants.reactive.take() {
                // Both change: each's latest, the outer's over the inner's.
                Some(original) => {
                    let (mut original, mut outer) = (original, outer);
                    let mut last: [[Option<Styles>; 4]; 2] = Default::default();
                    Box::new(move || {
                        let a = original();
                        let b = outer();
                        if a.is_none() && b.is_none() {
                            return None;
                        }
                        if let Some(a) = a {
                            last[0] = a;
                        }
                        if let Some(b) = b {
                            last[1] = b;
                        }
                        Some(std::array::from_fn(|i| match (&last[0][i], &last[1][i]) {
                            (Some(a), Some(b)) => {
                                let mut merged = a.clone();
                                merged.merge(b);
                                Some(merged)
                            }
                            (a, b) => b.clone().or_else(|| a.clone()),
                        }))
                    })
                }
                None => outer,
            });
        }
        if let Some(mut outer) = self.reactive_style {
            inner.reactive_style = Some(match inner.reactive_style.take() {
                Some(mut original) => Box::new(move |out: &mut Option<Styles>| {
                    original(out);
                    let mut over = None;
                    outer(&mut over);
                    if let (Some(style), Some(over)) = (out.as_mut(), over) {
                        style.merge(&over);
                    }
                }),
                None => outer,
            });
        }
        if self.disabled.is_some() {
            inner.disabled = self.disabled;
        }
        if self.click.is_some() {
            inner.click = self.click;
        }
        if self.id.is_some() {
            inner.id = self.id;
        }
        inner.mounted.extend(self.mounted);
        if let Some(rare) = self.rare {
            rare.merge_into(inner.rare_mut());
        }
        inner
    }
}
impl Styled for View {
    fn styles_mut(&mut self) -> &mut Styles {
        &mut self.styles
    }
}
impl From<&str> for View {
    fn from(value: &str) -> Self {
        text(value)
    }
}
impl From<String> for View {
    fn from(value: String) -> Self {
        text(value)
    }
}
/// Retained vector content. The callback tracks reactive reads and receives the
/// allocated content size. Drawing is clipped to that box; children are rejected.
pub fn canvas(draw: impl FnMut((f32, f32)) -> crate::canvas::Canvas + 'static) -> View {
    View::new(Kind::Canvas(Box::new(draw)))
}
/// Children side by side. As CSS's flexbox, they stretch to the row's
/// height unless they have one of their own (`items_*` aligns them
/// otherwise).
pub fn row() -> View {
    View::new(Kind::Container(Layout::Row)).items_stretch()
}
/// Children one above the other. As CSS's flexbox, they stretch to the
/// column's width unless they have one of their own (`items_*` aligns them
/// otherwise).
pub fn column() -> View {
    View::new(Kind::Container(Layout::Column)).items_stretch()
}
/// A [`column()`].
pub fn div() -> View {
    column()
}
/// Children stacked in one box, each laid out in all of it as CSS grid lays
/// out items sharing one area: sized against the overlay's own box, aligned
/// in it by `items` across and `justify` down, and by default stretched to
/// fill it (`items_stretch().justify_stretch()`), except along an axis a
/// child sizes itself on. Its size is its largest child's.
pub fn overlay() -> View {
    View::new(Kind::Container(Layout::Overlay))
        .items_stretch()
        .justify_stretch()
}
/// A control's default focus ring: an accent border. Where the control has
/// no border of its own it paints over the control (an inset stroke, taking
/// no layout room), so focusing never moves its content; otherwise it
/// colours the control's own border. (Made when first focused.)
fn focus_ring(styles: &Styles) -> fn() -> Styles {
    match bordered(styles) {
        true => || widget_style(|w| &w.focus_recolor),
        false => || widget_style(|w| &w.focus_ring),
    }
}
/// A menu item's focus: its hover wash and the focus ring, as `focus_ring`.
fn menu_focus(styles: &Styles) -> fn() -> Styles {
    match bordered(styles) {
        true => || widget_style(|w| &w.menu_focus_recolor),
        false => || widget_style(|w| &w.menu_focus),
    }
}
fn bordered(styles: &Styles) -> bool {
    styles.border_width.get().is_some_and(|width| width > 0.)
        || styles.border_edges.is_some_and(|edges| edges.is_some())
}
/// A scroll view's size when nothing else sizes it (`Style::intrinsic_width`).
/// One scrolling across is as tall as what it holds, as CSS's
/// `overflow-x: auto` box: a tab strip, a row of chips.
/// A canvas's text: a text node for each line, over its drawing and through
/// the same paint as any text. Made when the first line is, in a box of
/// their own that takes the canvas's pan and zoom. Scene nodes with no
/// semantics: the canvas's `label` speaks for what it shows.
#[derive(Default)]
struct CanvasLabels {
    group: Option<NodeId>,
    /// Each line's node, reused by position, and what it shows now: a line
    /// drawn as it was touches nothing, so a chart redrawn every frame
    /// costs what changed.
    nodes: Vec<(NodeId, crate::canvas::CanvasText)>,
    size: (f32, f32),
    view: Option<crate::scene::CanvasView>,
}
impl CanvasLabels {
    fn show(
        &mut self,
        scene: &mut crate::scene::Scene,
        root: NodeId,
        texts: &[crate::canvas::CanvasText],
        (width, height): (f32, f32),
    ) {
        let group = match self.group {
            Some(group) => group,
            None => {
                let group = scene.append(
                    root,
                    NodeKind::Container(Layout::Overlay),
                    crate::scene::Style {
                        absolute: true,
                        ..Default::default()
                    },
                );
                self.group = Some(group);
                if let Some(view) = self.view {
                    self.follow(scene, view);
                }
                group
            }
        };
        if self.size != (width, height) {
            self.size = (width, height);
            scene.set_style(
                group,
                crate::scene::Style {
                    width: Some(width),
                    height: Some(height),
                    absolute: true,
                    ..Default::default()
                },
            );
        }
        for (gone, _) in self.nodes.drain(texts.len().min(self.nodes.len())..) {
            scene.remove(gone);
        }
        for (index, text) in texts.iter().enumerate() {
            if self
                .nodes
                .get(index)
                .is_some_and(|(_, shown)| shown == text)
            {
                continue;
            }
            let kind = NodeKind::Text {
                text: text.text.clone(),
                color: text.color,
                font_size: text.size,
            };
            let node = match self.nodes.get_mut(index) {
                Some((node, shown)) => {
                    scene.set_kind(*node, kind);
                    *shown = text.clone();
                    *node
                }
                None => {
                    let node = scene.append(
                        group,
                        kind,
                        crate::scene::Style {
                            absolute: true,
                            ..Default::default()
                        },
                    );
                    self.nodes.push((node, text.clone()));
                    node
                }
            };
            scene.set_font(node, text.font.clone());
            let size = match (text.anchor, text.baseline) {
                (crate::canvas::TextAnchor::Start, crate::canvas::TextBaseline::Top) => (0., 0.),
                _ => scene.measure_text_with_font(&text.text, text.size, None, &text.font),
            };
            let (x, y) = text.origin(size);
            scene.set_transform(node, crate::scene::Transform { x, y });
        }
    }
    /// Pans and zooms the text with its canvas's drawing.
    fn follow(&mut self, scene: &mut crate::scene::Scene, view: crate::scene::CanvasView) {
        self.view = Some(view);
        if let Some(group) = self.group {
            scene.set_transform(
                group,
                crate::scene::Transform {
                    x: view.x,
                    y: view.y,
                },
            );
            scene.set_local_transform(
                group,
                crate::scene::LocalTransform {
                    scale: (view.scale, view.scale),
                    origin: (0., 0.),
                    ..crate::scene::LocalTransform::IDENTITY
                },
            );
        }
    }
}

fn scroll_default_size(horizontal: bool) -> crate::scene::Style {
    crate::scene::Style {
        width: Some(320.),
        height: (!horizontal).then_some(240.),
        intrinsic_width: true,
        intrinsic_height: !horizontal,
        ..Default::default()
    }
}
/// A line or paragraph of text, wrapping at its width.
pub fn text(value: impl Into<String>) -> View {
    View::new(Kind::Text(value.into()))
}
/// Text that follows what `compute` reads.
pub fn text_signal(compute: impl FnMut() -> String + 'static) -> View {
    View::new(Kind::BoundText(Box::new(compute)))
}
/// A localized message's text, following its locale, its file and its
/// arguments (`i18n.t("inbox.count").arg("n", count)`).
pub fn text_message(message: crate::i18n::Message) -> View {
    text_signal(move || message.get())
}
pub use crate::compose_context_menu::ContextMenuEvent;
/// Where anchored popovers and menus open (`View::placement`).
pub use crate::compose_modal::ModalPlacement;
pub use crate::compose_popover::{Edge as PlacementEdge, Placement, Side as PlacementSide};
pub use crate::compose_rich::{RichTextView, SpanHover, SpanIcon, TextSpan, rich_text, text_span};
pub use crate::compose_tooltip::Tooltip;
/// Reactive inline children; equal resolved content preserves paragraph layout/paint.
pub fn rich_text_signal(compute: impl FnMut() -> Vec<TextSpan> + 'static) -> View {
    View::new(Kind::RichText(Box::new(compute)))
}
pub(crate) fn with_rich_spans(mut view: View, spans: Vec<TextSpan>) -> View {
    view.kind = Kind::RichText(Box::new(move || spans.clone()));
    view
}
/// Decoded frames with owned, visibility-aware playback. Offscreen, clipped and
/// transparent allocations pause and remove their timer; removal cancels it.
pub fn animated_image(
    label: impl Into<String>,
    animation: std::sync::Arc<crate::animation::Animation>,
) -> View {
    let mut view = image(label, animation.frames()[0].image.clone());
    view.rare_mut().animation = Some(animation::Animated {
        animation,
        playing: None,
    });
    view
}
/// An [`animated_image`] that plays while `playing` is true.
pub fn animated_image_controlled(
    label: impl Into<String>,
    animation: std::sync::Arc<crate::animation::Animation>,
    playing: Signal<bool>,
) -> View {
    let mut view = animated_image(label, animation);
    view.rare_mut().animation.as_mut().unwrap().playing = Some(playing);
    view
}
/// A retained CoreVideo frame (see [`crate::native_surface`]), shown
/// without copying its pixels. `label` is its accessible name.
#[cfg(target_os = "macos")]
pub fn native_surface(
    label: impl Into<String>,
    source: std::rc::Rc<crate::native_surface::NativeSurface>,
) -> View {
    native_surface_signal(label, move || source.clone())
}
/// A [`native_surface`] whose surface follows what `source` reads.
#[cfg(target_os = "macos")]
pub fn native_surface_signal(
    label: impl Into<String>,
    source: impl FnMut() -> std::rc::Rc<crate::native_surface::NativeSurface> + 'static,
) -> View {
    View::new(Kind::NativeSurface {
        label: label.into(),
        source: Box::new(source),
    })
    .object_fit(crate::style::ObjectFit::Contain)
}
/// An immutable decoded image. Unspecified dimensions use the source pixel size;
/// explicit dimensions stretch its pixels into the allocated content box.
/// `label` is its accessible name.
pub fn image(label: impl Into<String>, source: std::sync::Arc<crate::image::ImageData>) -> View {
    image_signal(label, move || source.clone())
}
/// A retained image whose decoded source follows reactive reads in `source`.
/// Image views own their pixels and reject additional children.
/// Load with a shared bounded cache. The component owns its request; unmounting
/// cancels transport when no other component is waiting for the same key.
/// Give it a size before its pixels arrive (a width with `aspect_ratio`, as
/// an API or markdown often states them) and it lays out once: loading shows
/// in that box and the image fits it, so nothing around it moves.
pub fn async_image<
    F: Future<
            Output = Result<
                std::sync::Arc<crate::image::ImageData>,
                crate::image_cache::ImageLoadError,
            >,
        > + 'static,
>(
    label: impl Into<String>,
    cache: crate::image_cache::ImageCache,
    key: impl Into<String>,
    fetch: impl FnOnce() -> F + 'static,
    loading: View,
    mut error: impl FnMut(crate::image_cache::ImageLoadError) -> View + 'static,
) -> View {
    let label = label.into();
    let key = key.into();
    component(move |cx| {
        let state = cx.state(None);
        let write = state.clone();
        let request = cache.load(key, fetch);
        cx.tasks().spawn(async move {
            write.set(Some(request.await));
        });
        let mut loading = Some(loading);
        switch(
            move || state.get(),
            move |value, _| match value {
                None => loading.take().expect("initial loading view"),
                Some(Ok(source)) => image(label.clone(), source),
                Some(Err(failure)) => error(failure),
            },
        )
    })
}
/// SVG source parsed and rasterized by the renderer at the allocated device size.
/// Defaults to a 24-pixel square; size, tint and transforms retain their node.
pub fn svg(label: impl Into<String>, source: std::sync::Arc<crate::svg::SvgData>) -> View {
    svg_signal(label, move || source.clone())
}
/// An [`svg`] whose source follows what `source` reads.
pub fn svg_signal(
    label: impl Into<String>,
    source: impl FnMut() -> std::sync::Arc<crate::svg::SvgData> + 'static,
) -> View {
    View::new(Kind::Svg {
        label: label.into(),
        source: Box::new(source),
    })
    .size(24., 24.)
}
/// An [`image`] whose decoded source follows what `source` reads. Image
/// views own their pixels and reject children.
pub fn image_signal(
    label: impl Into<String>,
    source: impl FnMut() -> std::sync::Arc<crate::image::ImageData> + 'static,
) -> View {
    View::new(Kind::Image {
        label: label.into(),
        source: Box::new(source),
    })
}
/// A box whose pixels `view` draws on the GPU (see [`crate::gpu_view`]):
/// a frame at the box's size as it first shows, as it resizes, as the view
/// is invalidated and, while it animates, every refresh it shows. It has no
/// size of its own: give it one. `label` is its accessible name.
pub fn gpu_view(label: impl Into<String>, view: &crate::gpu_view::GpuView) -> View {
    // A box mounted as any is, its frames set up once it is: no kind of its
    // own for every element's mount to tell apart.
    let (label, view) = (label.into(), view.clone());
    component(move |cx| {
        let runner = cx.try_service::<TaskRunner>();
        let frames = cx.try_service::<crate::frame::FrameClock>();
        // Clipping its frame: its own corners cut it, as `rounded` says.
        div().overflow_hidden().on_mount(move |handle| {
            if let Some(mut ui) = handle.ui.upgrade() {
                ui.scene.borrow_mut().set_replaced(handle.node, true);
                ui.semantics
                    .borrow_mut()
                    .set(handle.node, SemanticNode::new(Role::Image, label));
                mount_gpu_view(&mut ui, handle.node, view, runner, frames);
            }
        })
    })
}
/// A vertical viewport for ordinary retained children, stacked in a column.
/// Child geometry determines its scroll extent; `offset` is clamped after layout.
pub fn scroll(offset: Signal<f32>) -> View {
    View::new(Kind::Scroll {
        offset,
        horizontal: false,
    })
}
/// A horizontal viewport for retained children, arranged in a row.
/// Only horizontal wheel deltas are consumed; vertical scrolling bubbles.
pub fn scroll_x(offset: Signal<f32>) -> View {
    View::new(Kind::Scroll {
        offset,
        horizontal: true,
    })
}
/// A signal-controlled modal portal. Children and fluent styles describe its panel.
pub fn modal(label: impl Into<String>, open: Signal<bool>) -> View {
    View::new(Kind::Modal {
        label: label.into(),
        open,
        anchor: None,
    })
}
/// An anchored popup with retained panel children. The anchor stays in ordinary layout.
pub fn popover(label: impl Into<String>, open: Signal<bool>, anchor: View) -> View {
    View::new(Kind::Modal {
        label: label.into(),
        open,
        anchor: Some(Box::new(anchor)),
    })
}
/// An anchored menu whose retained children may be components or keyed menu items.
pub fn menu(label: impl Into<String>, open: Signal<bool>, anchor: View) -> View {
    View::new(Kind::Menu {
        label: label.into(),
        open,
        anchor: Box::new(anchor),
    })
}
/// A side-opening child menu with an automatically owned menu-item trigger.
pub fn submenu(label: impl Into<String>, open: Signal<bool>) -> View {
    View::new(Kind::Submenu {
        label: label.into(),
        open,
    })
}
/// A menu action. Activation closes its menu before running `.on_click`.
/// It shows `label`, or the children given it (`label` then names it).
pub fn menu_item(label: impl Into<String>) -> View {
    View::new(Kind::MenuItem(label.into()))
}
/// A push button holding its children. `.on_click` runs when it is
/// clicked, or pressed from the keyboard.
pub fn button() -> View {
    View::new(Kind::Button)
}
/// A radio button: chooses `value` for `selected` when clicked or pressed
/// (Space), and is checked while `selected` holds it. Its circle is drawn as
/// the platform draws one, beside `label` (also its accessible name). In a
/// [`radio_group`] its group is one Tab stop, landing on the checked radio,
/// and the arrows move and choose, wrapping at the ends, as a browser's and
/// AppKit's radio buttons do.
pub fn radio<T: PartialEq + Clone + 'static>(
    label: impl Into<String>,
    selected: Signal<T>,
    value: T,
) -> View {
    use crate::theme::tok;
    let label = label.into();
    let on = {
        let (selected, value) = (selected.clone(), value.clone());
        move || selected.with(|s| *s == value)
    };
    let (ring, dot, checked) = (on.clone(), on.clone(), on);
    let circle = div()
        .w(crate::compose_checkbox::SIZE)
        .h(crate::compose_checkbox::SIZE)
        .shrink_0()
        .rounded(crate::compose_checkbox::SIZE / 2.)
        .items_center()
        .justify_center()
        .reactive_style(move || {
            if ring() {
                Styles::new()
                    .bg(tok::ACCENT)
                    .border(1.)
                    .border_color(tok::ACCENT)
            } else {
                Styles::new()
                    .bg(tok::INPUT_BG)
                    .border(1.)
                    .border_color(tok::BORDER_STRONG)
            }
        })
        .child(div().w(6.).h(6.).rounded(3.).reactive_style(move || {
            if dot() {
                Styles::new().bg(tok::ACCENT_TEXT)
            } else {
                Styles::new().bg(crate::scene::Color(0, 0, 0, 0))
            }
        }));
    button()
        .role(Role::RadioButton)
        .accessible_label(label.clone())
        .checked_when(checked)
        .gap(6.)
        .items_center()
        .px(4.)
        .py(4.)
        .bg(crate::scene::Color(0, 0, 0, 0))
        .border(0.)
        .on_click(move || {
            if !selected.with_untracked(|s| *s == value) {
                selected.set(value.clone());
            }
        })
        .child(circle)
        .child(text(label))
}
/// Radio buttons ([`radio`]) of one choice, `label` its accessible name:
/// one Tab stop, the arrows moving and choosing (`Role::RadioGroup`).
pub fn radio_group(label: impl Into<String>) -> View {
    column()
        .role(Role::RadioGroup)
        .accessible_label(label)
        .gap(2.)
}
/// A checkbox bound bidirectionally to `value`.
/// `label` supplies both its visible text and accessible name. The control owns
/// its indicator and label; additional `.child`/`.children` are rejected at mount.
/// An optional `.on_click` callback runs after the value has toggled.
pub fn checkbox(label: impl Into<String>, value: Signal<bool>) -> View {
    View::new(Kind::Checkbox {
        label: label.into(),
        value,
        mixed: None,
    })
}
/// A progress indicator: values normalize to 0..=1; NaN is indeterminate
/// (work of unknown size), a stretch of the fill sweeping across.
/// The accessible label is not drawn. This leaf owns its clipped fill;
/// `text_color` controls the fill color and `bg` controls the track.
pub fn progress(label: impl Into<String>, value: Signal<f32>) -> View {
    View::new(Kind::Progress {
        label: label.into(),
        value,
    })
}
/// A horizontal numeric control bound bidirectionally to `value`.
/// The finite range must be increasing. This leaf owns its rail and thumb;
/// `label` supplies the accessible name, and typography color styles the thumb.
pub fn slider(
    label: impl Into<String>,
    value: Signal<f32>,
    range: std::ops::RangeInclusive<f32>,
) -> View {
    assert!(
        range.start().is_finite() && range.end().is_finite() && range.start() < range.end(),
        "slider range must be finite and increasing"
    );
    View::new(Kind::Slider {
        label: label.into(),
        value,
        range,
    })
}
/// A single-line editor bound bidirectionally to `value`.
/// `label` is its accessible name; dimensions and typography use ordinary styles.
pub fn text_input(label: impl Into<String>, value: Signal<String>) -> View {
    editor_view(label.into(), value.into(), false)
}
/// A multiline editor with selection, undo, clipboard and IME support.
pub fn text_area(label: impl Into<String>, value: Signal<String>) -> View {
    editor_view(label.into(), value.into(), true)
}
/// `text_input` bound to a [`TextValue`](crate::text::TextValue): a long text
/// is shared with the editor (its layout and accessibility too), not copied,
/// and an edit's new value shares every chunk the edit did not touch.
pub fn text_input_value(label: impl Into<String>, value: Signal<crate::text::TextValue>) -> View {
    editor_view(label.into(), value.into(), false)
}
/// `text_area` bound to a [`TextValue`](crate::text::TextValue), as
/// `text_input_value`: for documents.
pub fn text_area_value(label: impl Into<String>, value: Signal<crate::text::TextValue>) -> View {
    editor_view(label.into(), value.into(), true)
}
/// An editor bound to either kind of value (as `text_input` or `text_area`).
pub fn editor_view(label: String, value: crate::widgets::EditorValue, multiline: bool) -> View {
    View::new(Kind::Editor {
        label,
        value,
        multiline,
    })
}
/// A component: `build` runs once, as it mounts, with a [`Context`] for its
/// state, services and tasks, and returns its view.
pub fn component(build: impl FnOnce(&mut Context) -> View + 'static) -> View {
    View::new(Kind::Component(Box::new(build)))
}
/// `child`, with `value` provided to its subtree: components inside read it
/// with [`Context::service`].
pub fn provide<T: 'static>(value: T, child: View) -> View {
    provide_with(move |_| value, child)
}
/// [`provide`], the value made as `child` mounts, from the providing
/// context.
pub fn provide_with<T: 'static>(
    factory: impl FnOnce(&mut Context) -> T + 'static,
    child: View,
) -> View {
    View::new(Kind::Provider(
        Box::new(move |cx| {
            let value = factory(cx);
            cx.environment.services.provide(value);
        }),
        Box::new(child),
    ))
}
thread_local! {
    /// Whether any `FollowedLabel` was made: until then a component's
    /// `FollowedLabel::take` looks for none.
    static FOLLOWED: Cell<bool> = const { Cell::new(false) };
}
/// A label its caller follows (a translation, a state), for a component
/// that names one of its inner nodes with its `label` (a tab strip, a
/// select's button): provided around it ([`FollowedLabel::around`]) and
/// taken by it as it builds ([`FollowedLabel::take`]), so a component
/// inside it takes none. Components given a plain `String` look for none.
pub struct FollowedLabel(RefCell<Option<Followed>>);
/// A label that changes, as [`FollowedLabel`] gives it: shared, so a
/// component can name more than one node with it.
#[derive(Clone)]
pub struct Followed(Rc<RefCell<Box<dyn FnMut() -> String>>>);
impl Followed {
    /// What it says now (a tracked read, in an effect).
    pub fn get(&self) -> String {
        (self.0.borrow_mut())()
    }
}
impl FollowedLabel {
    /// `view` (a component), its label following `label`.
    pub fn around(label: impl FnMut() -> String + 'static, view: View) -> View {
        FOLLOWED.set(true);
        let label: Box<dyn FnMut() -> String> = Box::new(label);
        provide(
            FollowedLabel(RefCell::new(Some(Followed(Rc::new(RefCell::new(label)))))),
            view,
        )
    }
    /// The label provided around the component building in `cx`, if any:
    /// its own, so components inside it get none.
    pub fn take(cx: &Context) -> Option<Followed> {
        if !FOLLOWED.get() {
            return None;
        }
        cx.try_service::<FollowedLabel>()?.0.borrow_mut().take()
    }
}

/// `child` under a theme scope that changes `over` (another theme, some
/// tokens) and follows the enclosing scope otherwise: a light card in a
/// dark app, a subtree with its own accent. Overlays the subtree opens
/// (menus, popovers) inherit it.
pub fn theme_scope(over: crate::theme::ThemeOverride, child: View) -> View {
    View::new(Kind::Provider(
        Box::new(move |cx| {
            cx.environment.theme = cx.environment.theme.child(over);
        }),
        Box::new(child),
    ))
}
/// As [`theme_scope`], with what is overridden computed and kept current
/// (its reads are tracked): `|| ThemeOverride::new().theme(if dark.get() {..} else {..})`.
pub fn theme_scope_with(
    mut over: impl FnMut() -> crate::theme::ThemeOverride + 'static,
    child: View,
) -> View {
    View::new(Kind::Provider(
        Box::new(move |cx| {
            let initial = over();
            let scope = cx.environment.theme.child(initial);
            let target = scope.clone();
            let mut first = true;
            let effect = cx.runtime.effect(move || {
                let next = over();
                if !std::mem::take(&mut first) {
                    target.set_override(next);
                }
            });
            cx.retain(effect);
            cx.environment.theme = scope;
        }),
        Box::new(child),
    ))
}
/// A vertical scrollbar for a scroll view, drawn apart from it: place it
/// over the view's right edge, after anything that should not cover it (a
/// frosted bar the view scrolls under). `offset` is the view's scroll offset
/// (as `scroll`'s or a virtual list's) and `extent` its content height; the
/// view's length is the bar's own height.
pub fn scrollbar(offset: Signal<f32>, mut extent: impl FnMut() -> f32 + 'static) -> View {
    View::new(Kind::Region(Box::new(move |ui, root, _| {
        let size = ui.observe_content_size(root);
        let length = ui.signal(0_f32);
        let write = length.clone();
        ui.bind(root, move || {
            write.set(extent());
        });
        crate::compose_scrollbar::mount(ui, root, offset, size, length, false);
    })))
}
/// A conditional region. Equal selector values retain their component and local state.
pub fn switch<K: PartialEq + Clone + 'static>(
    mut select: impl FnMut() -> K + 'static,
    mut build: impl FnMut(K, &mut Context) -> View + 'static,
) -> View {
    View::new(Kind::Region(Box::new(move |ui, root, environment| {
        let weak = ui.downgrade();
        let runtime = ui.runtime.clone();
        let mut selected: Option<K> = None;
        let mut mounted = None;
        ui.bind(root, move || {
            let key = select();
            if selected.as_ref() == Some(&key) {
                return;
            }
            let Some(mut ui) = weak.upgrade() else {
                return;
            };
            if !ui.scene.borrow().contains(root) {
                return;
            }
            runtime.untracked(|| {
                let mut cx = Context::new(runtime.clone(), environment.clone());
                let view = build(key.clone(), &mut cx);
                let mut transaction = MountTransaction::new(&ui, root);
                let node = mount(&mut ui, root, view, environment.clone());
                cx.finish(&mut ui, node);
                transaction.commit();
                if let Some(old) = mounted.replace(node) {
                    ui.remove(old);
                }
            });
            selected = Some(key);
        });
    })))
}
/// Keyed children retain their local state and subscriptions across reorder.
/// Duplicate keys are rejected before mutating the mounted children.
/// Constructor panics clean up new children and preserve the previous mounted
/// set. Application state written by a constructor is not rolled back.
pub fn keyed<K: Eq + std::hash::Hash + Clone + 'static>(
    mut keys: impl FnMut() -> Vec<K> + 'static,
    mut build: impl FnMut(K, &mut Context) -> View + 'static,
) -> View {
    View::new(Kind::Region(Box::new(move |ui, root, environment| {
        let weak = ui.downgrade();
        let runtime = ui.runtime.clone();
        let mut entries: HashMap<K, NodeId, crate::KeyHasher> =
            HashMap::with_hasher(crate::key_hasher());
        // The last pass's keys and their children, in order.
        let mut last: (Vec<K>, Vec<NodeId>) = (Vec::new(), Vec::new());
        ui.bind(root, move || {
            let keys = keys();
            // The same keys in the same order, every child still mounted (a
            // list whose rows changed in place): nothing to add, remove or
            // move, and no key hashed to find out.
            if keys == last.0
                && let Some(ui) = weak.upgrade()
            {
                let scene = ui.scene.borrow();
                if !scene.contains(root) || last.1.iter().all(|node| scene.contains(*node)) {
                    return;
                }
            }
            let mut unique = std::collections::HashSet::with_capacity_and_hasher(
                keys.len(),
                crate::key_hasher(),
            );
            unique.extend(keys.iter());
            assert_eq!(unique.len(), keys.len(), "duplicate component child key");
            let Some(mut ui) = weak.upgrade() else {
                return;
            };
            if !ui.scene.borrow().contains(root) {
                return;
            }
            let order = runtime.untracked(|| {
                let mut transaction = MountTransaction::new(&ui, root);
                // Stage every new child before changing the committed registry
                // or removing old children. The mount guard disposes staged
                // roots if any later constructor panics.
                let mut added = Vec::new();
                for key in &keys {
                    if entries
                        .get(key)
                        .is_some_and(|node| ui.scene.borrow().contains(*node))
                    {
                        continue;
                    }
                    let mut cx = Context::new(runtime.clone(), environment.clone());
                    let view = build(key.clone(), &mut cx);
                    let node = mount(&mut ui, root, view, environment.clone());
                    cx.finish(&mut ui, node);
                    added.push((key.clone(), node));
                }
                entries.extend(added);
                // Every key shown now is held: only more than that left.
                // Those go in the order they were in, the same every run (a
                // map's iteration is not), as what their removal repaints
                // follows it.
                if entries.len() > unique.len() {
                    for (index, key) in last.0.iter().enumerate() {
                        if !unique.contains(key)
                            && let Some(node) = entries.remove(key)
                        {
                            ui.remove_indexed(node, index);
                        }
                    }
                    // Any not in the last order (left by a pass that did not finish).
                    entries.retain(|key, node| {
                        if unique.contains(key) {
                            true
                        } else {
                            ui.remove_in_effect(*node);
                            false
                        }
                    });
                }
                let order: Vec<_> = keys.iter().map(|key| entries[key]).collect();
                assert!(ui.scene.borrow_mut().reorder_children(root, &order));
                transaction.commit();
                order
            });
            last = (keys, order);
        });
    })))
}
/// What builds a virtual list's row: its index (a signal following it),
/// its key and the row's context.
type RowBuilder<K> = dyn FnMut(Signal<usize>, K, &mut Context) -> View;
/// A virtual list's key and row builders, shared by the list and the text
/// a copy takes of rows it has not mounted (`View::row_text`).
pub(crate) struct RowSource<K> {
    key: Rc<RefCell<dyn FnMut(usize) -> K>>,
    build: Rc<RefCell<RowBuilder<K>>>,
    /// The runtime and environment the list mounted in: rows are built in
    /// them.
    pub(crate) mounted_in: Rc<RefCell<Option<(Runtime, Environment)>>>,
}
impl<K: 'static> RowSource<K> {
    pub(crate) fn new(
        key: impl FnMut(usize) -> K + 'static,
        build: impl FnMut(Signal<usize>, K, &mut Context) -> View + 'static,
    ) -> Self {
        Self {
            key: Rc::new(RefCell::new(key)),
            build: Rc::new(RefCell::new(build)),
            mounted_in: Rc::default(),
        }
    }
    pub(crate) fn key(&self) -> impl FnMut(usize) -> K + 'static {
        let key = self.key.clone();
        move |index| (key.borrow_mut())(index)
    }
    pub(crate) fn build(&self) -> impl FnMut(Signal<usize>, K, &mut Context) -> View + 'static {
        let build = self.build.clone();
        move |index, key, cx| (build.borrow_mut())(index, key, cx)
    }
    /// Row `index`'s text, of `len()` rows: the row built as the list builds
    /// it, not mounted (what it makes ends at once), and read (`view_text`).
    pub(crate) fn text(
        &self,
        len: impl Fn() -> usize + 'static,
    ) -> impl Fn(usize) -> Option<String> + 'static {
        let (key, build, mounted_in) = (
            self.key.clone(),
            self.build.clone(),
            self.mounted_in.clone(),
        );
        move |index| {
            if index >= len() {
                return None;
            }
            let (runtime, environment) = mounted_in.borrow().clone()?;
            // Built now (a copy during a row's build): copied as nothing.
            let row = key.try_borrow_mut().ok().map(|mut key| key(index))?;
            let mut cx = Context::new(runtime.clone(), environment);
            let mut view = build
                .try_borrow_mut()
                .ok()
                .map(|mut build| build(runtime.signal(index), row, &mut cx))?;
            Some(view_text(&mut view))
        }
    }
}
/// The text `view` shows, as a copy of it takes it: its texts in order, a
/// tab between a row's children and a line break between others'. What
/// it builds when mounted (components, regions) is left out.
pub(crate) fn view_text(view: &mut View) -> String {
    match &mut view.kind {
        Kind::Text(text) => return text.clone(),
        Kind::BoundText(text) => return text(),
        Kind::RichText(spans) => return spans().iter().map(|span| &*span.text).collect(),
        _ => {}
    }
    let separator = if matches!(view.kind, Kind::Container(Layout::Row)) {
        "\t"
    } else {
        "\n"
    };
    let parts: Vec<String> = view
        .children
        .iter_mut()
        .map(view_text)
        .filter(|text| !text.is_empty())
        .collect();
    parts.join(separator)
}
/// Where a row scrolled to shows in its list's viewport
/// (`NodeHandle::scroll_to_row`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RowAlign {
    /// Its top at the viewport's top.
    Start,
    /// Its middle at the viewport's middle.
    Center,
    /// Its bottom at the viewport's bottom.
    End,
    /// As little scrolling as shows it all (none if it does): the keyboard's.
    #[default]
    Nearest,
}
impl RowAlign {
    /// The scroll position that shows the row at `top`, `height` tall, in a
    /// viewport `view` tall now at `position`, before clamping.
    pub(crate) fn position(self, top: f32, height: f32, view: f32, position: f32) -> f32 {
        let bottom = (top as f64 + height as f64).min(f32::MAX as f64) as f32;
        match self {
            RowAlign::Start => top,
            RowAlign::End => bottom - view,
            RowAlign::Center => top + height / 2. - view / 2.,
            RowAlign::Nearest if top < position || height > view => top,
            RowAlign::Nearest if bottom as f64 > position as f64 + view as f64 => bottom - view,
            RowAlign::Nearest => position,
        }
    }
}
/// A list's scroll-to-row request: a serial (each request new), the row,
/// and where it shows.
pub(crate) type RowReveal = (u64, usize, RowAlign);
/// The list at `root` takes scroll-to-row requests (`NodeHandle::scroll_to_row`)
/// on the signal this returns, while it is mounted.
/// `row_index`: a mounted row's index from its place down the list's
/// content (find in page tells its rows apart by it).
pub(crate) fn list_reveal(
    ui: &mut Ui,
    root: NodeId,
    row_index: crate::find::RowIndex,
) -> Signal<RowReveal> {
    let reveal = ui.signal((0_u64, 0_usize, RowAlign::Nearest));
    ui.list_reveals().borrow_mut().insert(root, reveal.clone());
    ui.storage.list_rows.borrow_mut().insert(root, row_index);
    struct Forget(WeakUi, NodeId);
    impl Drop for Forget {
        fn drop(&mut self) {
            if let Some(ui) = self.0.upgrade() {
                if let Ok(mut reveals) = ui.list_reveals().try_borrow_mut() {
                    reveals.remove(&self.1);
                }
                if let Ok(mut rows) = ui.storage.list_rows.try_borrow_mut() {
                    rows.remove(&self.1);
                }
            }
        }
    }
    let forget = Forget(ui.downgrade(), root);
    ui.retain(root, forget);
    reveal
}
/// Fixed-height keyed rows; only the visible range plus overscan is mounted.
/// `count` and `key` track reactive reads. `build` runs once per mounted key;
/// its index signal updates when that key moves. Offscreen rows are disposed,
/// so persistent row state belongs in the model. Keys must be unique among
/// visible rows. `offset` is a bidirectional logical-pixel scroll position.
pub fn virtual_list<K: Eq + std::hash::Hash + Clone + 'static>(
    offset: Signal<f32>,
    row_height: f32,
    overscan: usize,
    count: impl FnMut() -> usize + 'static,
    key: impl FnMut(usize) -> K + 'static,
    build: impl FnMut(Signal<usize>, K, &mut Context) -> View + 'static,
) -> View {
    assert!(row_height.is_finite() && row_height > 0.0);
    let count = Rc::new(RefCell::new(count));
    let rows = RowSource::new(key, build);
    let text = rows.text({
        let count = count.clone();
        move || count.try_borrow_mut().map_or(0, |mut count| count())
    });
    let (mut key, mut build, mounted_in) = (rows.key(), rows.build(), rows.mounted_in.clone());
    let count = move || (count.borrow_mut())();
    View::new(Kind::VirtualList(Box::new(
        move |ui, root, environment, options: ListOptions| {
            let ListOptions {
                scrollbar,
                keyboard_navigation,
                contain,
                edges,
            } = options;
            *mounted_in.borrow_mut() = Some((ui.runtime.clone(), environment.clone()));
            let size = ui.observe_content_size(root);
            let extent = ui.signal(0.0_f32);
            if let Some(edges) = edges {
                watch_edges(
                    ui,
                    root,
                    offset.clone(),
                    size.clone(),
                    extent.clone(),
                    *edges,
                );
            }
            // Clip rows to the allocated content box, keeping root padding clear.
            let viewport = ui.container(root, Layout::Overlay, Default::default());
            let content = ui.container(viewport, Layout::Overlay, Default::default());
            ui.scene.borrow_mut().set_scroll_copy(content, true);
            let stretch = ListStretch::new(
                ui,
                root,
                content,
                offset.clone(),
                size.clone(),
                extent.clone(),
            );
            let navigation = keyboard_navigation.then(|| {
                crate::compose_virtual_keyboard::Navigation::new(
                    ui,
                    root,
                    offset.clone(),
                    size.clone(),
                    row_height,
                )
            });
            let wheel_offset = offset.clone();
            let wheel_size = size.clone();
            let wheel_extent = extent.clone();
            ui.on_event(root, false, move |cx| {
                if cx.phase == EventPhase::Capture || cx.default_prevented() {
                    return;
                }
                if let InputEvent::Scroll { delta_y, .. } = cx.event {
                    // A Control-wheel zooms the window instead.
                    if !delta_y.is_finite() || crate::zoom::wheel_zooms() {
                        return;
                    }
                    let before = wheel_offset.get();
                    let after = (before + delta_y)
                        .max(0.0)
                        .min((wheel_extent.get() - wheel_size.get().1).max(0.0));
                    if before != after {
                        wheel_offset.set(after);
                        cx.scrolled(false);
                    } else if contain && delta_y != 0. {
                        cx.scrolled(false);
                    }
                }
            });
            if scrollbar {
                crate::compose_scrollbar::mount(
                    ui,
                    root,
                    offset.clone(),
                    size.clone(),
                    extent.clone(),
                    false,
                );
            }
            let weak = ui.downgrade();
            let runtime = ui.runtime.clone();
            let mut entries: HashMap<
                K,
                (NodeId, Signal<usize>, Placed),
                rustc_hash::FxBuildHasher,
            > = HashMap::default();
            // Each run's number, which the rows it shows are stamped with.
            let mut run = 0_u64;
            let (mut keys, mut order) = (Vec::new(), Vec::new());
            let mut handled_request = 0;
            // Scroll-to-row requests (`NodeHandle::scroll_to_row`).
            let reveal = list_reveal(
                ui,
                root,
                Rc::new(move |y: f32| Some((y / row_height).round().max(0.) as usize)),
            );
            let mut handled_reveal = 0;
            ui.bind(root, move || {
                let length = count();
                let (width, height) = size.get();
                let model = crate::virtual_list::VirtualList::new(length, row_height, overscan);
                let requested = offset.get();
                let mut position = if requested.is_finite() {
                    requested.max(0.0)
                } else {
                    0.0
                }
                .min((model.content_height() - height).max(0.0));
                let request = navigation
                    .as_ref()
                    .map(|navigation| navigation.request.get());
                let target = request
                    .filter(|(serial, _)| *serial != handled_request)
                    .and_then(|(_, index)| {
                        (length > 0).then_some(index.min(length.saturating_sub(1)))
                    });
                // A row asked for, placed as asked; else the keyboard's.
                let (serial, row, align) = reveal.get();
                let asked = (serial != handled_reveal && length > 0)
                    .then_some((row.min(length.saturating_sub(1)), align));
                handled_reveal = serial;
                if let Some((index, align)) = asked.or(target.map(|i| (i, RowAlign::Nearest))) {
                    let top = model.row_offset(index);
                    position = align
                        .position(top, row_height, height, position)
                        .max(0.)
                        .min((model.content_height() - height).max(0.));
                }
                let range = model.visible_range(position, height);
                keys.clear();
                keys.extend(range.map(|index| (key(index), index)));
                // Rows kept are stamped with this run: one shown twice is a
                // key repeated, and those left unstamped go below.
                run += 1;
                let mut fresh = 0;
                for (key, _) in &keys {
                    match entries.get_mut(key) {
                        Some((.., placed)) => {
                            assert!(placed.run != run, "duplicate virtual list child key");
                            placed.run = run;
                        }
                        None => fresh += 1,
                    }
                }
                if fresh > 1 {
                    let mut new: std::collections::HashSet<&K, rustc_hash::FxBuildHasher> =
                        Default::default();
                    for (key, _) in &keys {
                        if !entries.contains_key(key) {
                            assert!(new.insert(key), "duplicate virtual list child key");
                        }
                    }
                }
                let Some(mut ui) = weak.upgrade() else {
                    return;
                };
                if !ui.scene.borrow().contains(root) {
                    return;
                }
                runtime.untracked(|| {
                    let _theme = ui.enter_theme(&environment.theme);
                    let mut transaction = MountTransaction::new(&ui, content);
                    // Construct first: a failed row constructor leaves old rows intact.
                    if fresh != 0 {
                        let mut added = Vec::new();
                        for (key, index) in &keys {
                            if entries.contains_key(key) {
                                continue;
                            }
                            let index_signal = ui.signal(*index);
                            let mut cx = Context::new(runtime.clone(), environment.clone());
                            let view = build(index_signal.clone(), key.clone(), &mut cx);
                            let row = ui.container(content, Layout::Overlay, Default::default());
                            crate::selection::mount_row(&mut ui, row, &index_signal);
                            let node = mount(&mut ui, row, view, environment.clone());
                            cx.finish(&mut ui, node);
                            if let Some(navigation) = &navigation {
                                navigation.mount_row(&mut ui, row, row_height);
                            }
                            added
                                .push((key.clone(), (row, index_signal, Placed { run, at: None })));
                        }
                        entries.extend(added);
                    }
                    let mut removed_focus = false;
                    if entries.len() > keys.len() {
                        entries.retain(|_, (node, _, placed)| {
                            if placed.run == run {
                                true
                            } else {
                                if navigation.is_some()
                                    && let Some(focused) = ui.input.focused()
                                {
                                    let scene = ui.scene.borrow();
                                    removed_focus |= focused == *node
                                        || (scene.contains(focused)
                                            && scene
                                                .ancestors(focused)
                                                .any(|ancestor| ancestor == *node));
                                }
                                ui.remove(*node);
                                false
                            }
                        });
                    }
                    for (key, index) in &keys {
                        entries[key].1.set(*index);
                    }
                    {
                        let mut scene = ui.scene.borrow_mut();
                        scene.set_style(
                            viewport,
                            crate::scene::Style {
                                clip: true,
                                ..crate::widgets::fixed(width, height)
                            },
                        );
                        scene.set_style(
                            content,
                            crate::widgets::fixed(width, model.content_height()),
                        );
                        stretch.place(&mut scene, position);
                        order.clear();
                        for (key, index) in &keys {
                            let (node, _, placed) = entries.get_mut(key).expect("a row");
                            // A row placed as it was, at the same width, stays.
                            let at = Some((*index, width.to_bits()));
                            if placed.at != at {
                                placed.at = at;
                                // Its root fills its width, as a block does.
                                scene.set_style(
                                    *node,
                                    crate::scene::Style {
                                        align: crate::scene::Align::Stretch,
                                        ..crate::widgets::fixed(width, row_height)
                                    },
                                );
                                scene.set_transform(
                                    *node,
                                    crate::scene::Transform {
                                        x: 0.0,
                                        y: model.row_offset(*index),
                                    },
                                );
                            }
                            order.push(*node);
                        }
                        assert!(scene.reorder_children(content, &order));
                    }
                    transaction.commit();
                    extent.set(model.content_height());
                    let unchanged_request = offset.get().to_bits() == requested.to_bits();
                    if unchanged_request {
                        offset.set(position);
                    }
                    if let Some(navigation) = &navigation {
                        navigation.update(
                            &ui,
                            root,
                            length,
                            keys.iter()
                                .map(|(key, index)| (*index, entries[key].0))
                                .collect(),
                        );
                        if let Some((serial, _)) = request {
                            handled_request = serial;
                        }
                        if let Some(index) = target.filter(|_| unchanged_request) {
                            if let Some((key, _)) =
                                keys.iter().find(|(_, candidate)| *candidate == index)
                            {
                                let item = navigation.item(entries[key].0);
                                ui.input.focus(&ui.scene, Some(item));
                            }
                        } else if removed_focus && ui.input.focused().is_none() {
                            ui.input.focus(&ui.scene, Some(root));
                        }
                    }
                });
            });
        },
    )))
    .row_text(text)
}
// Roll back newly appended roots if a component constructor or initial binding
// panics. Existing siblings are never removed by this guard. This is ownership
// cleanup, not rollback of arbitrary application state writes.
struct MountTransaction {
    ui: WeakUi,
    parent: NodeId,
    before: Vec<NodeId>,
    committed: bool,
}
impl MountTransaction {
    fn new(ui: &Ui, parent: NodeId) -> Self {
        Self {
            ui: ui.downgrade(),
            parent,
            before: ui.scene.borrow().children(parent).to_vec(),
            committed: false,
        }
    }
    fn commit(&mut self) {
        self.committed = true;
    }
}
impl Drop for MountTransaction {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        let Some(mut ui) = self.ui.upgrade() else {
            return;
        };
        if !ui.scene.borrow().contains(self.parent) {
            return;
        }
        let children = ui.scene.borrow().children(self.parent).to_vec();
        for child in children {
            if !self.before.contains(&child) {
                ui.remove(child);
            }
        }
    }
}

struct IdRegistration {
    registry: Registry,
    id: String,
    node: NodeId,
}
impl Drop for IdRegistration {
    fn drop(&mut self) {
        let mut registry = self.registry.borrow_mut();
        if let Some(nodes) = registry.get_mut(&self.id) {
            nodes.retain(|id| *id != self.node);
            if nodes.is_empty() {
                registry.remove(&self.id);
            }
        }
    }
}
pub(crate) fn mount(
    ui: &mut Ui,
    parent: NodeId,
    mut view: View,
    environment: Environment,
) -> NodeId {
    let depth = ui.mount_depth();
    if crate::stack_low(depth) {
        return stacker::grow(crate::STACK_SEGMENT, || {
            mount(ui, parent, view, environment)
        });
    }
    // Extract closures while retaining the root refinements on their wrapper.
    let kind = std::mem::replace(&mut view.kind, Kind::Container(Layout::Column));
    // Each kind mounts in a function of its own: mounting recurses once per
    // view, and in a debug build every value a function holds takes stack.
    ui.set_mount_depth(depth + 1);
    let root = match kind {
        Kind::Component(build) => mount_component(ui, parent, build, view, environment),
        Kind::Provider(provide, child) => {
            mount_provider(ui, parent, provide, *child, view, environment)
        }
        Kind::Slot(slot) => mount_slot(ui, parent, slot, view, environment),
        kind @ (Kind::Modal { .. } | Kind::Menu { .. } | Kind::Submenu { .. }) => {
            mount_overlay(ui, parent, kind, view, environment)
        }
        primitive => mount_element(ui, parent, primitive, view, environment),
    };
    ui.set_mount_depth(depth);
    root
}
#[cfg_attr(debug_assertions, inline(never))]
fn mount_component(
    ui: &mut Ui,
    parent: NodeId,
    build: Builder,
    view: View,
    environment: Environment,
) -> NodeId {
    let mut cx = Context::new(ui.runtime.clone(), environment.clone());
    let inner = ui.runtime.untracked(|| build(&mut cx));
    let root = mount(ui, parent, view.refine(inner), environment);
    cx.finish(ui, root);
    root
}
#[cfg_attr(debug_assertions, inline(never))]
fn mount_provider(
    ui: &mut Ui,
    parent: NodeId,
    provide: Box<dyn FnOnce(&mut Context)>,
    child: View,
    view: View,
    mut environment: Environment,
) -> NodeId {
    environment.services = environment.services.child();
    let mut cx = Context::new(ui.runtime.clone(), environment);
    ui.runtime.untracked(|| provide(&mut cx));
    let environment = cx.environment.clone();
    let root = mount(ui, parent, view.refine(child), environment);
    cx.finish(ui, root);
    root
}
#[cfg_attr(debug_assertions, inline(never))]
fn mount_slot(
    ui: &mut Ui,
    parent: NodeId,
    slot: Slot,
    view: View,
    mut environment: Environment,
) -> NodeId {
    environment.services = slot.services;
    let mut cx = Context::new(ui.runtime.clone(), environment.clone());
    let inner = ui.runtime.untracked(|| (slot.build)(&mut cx));
    let root = mount(ui, parent, view.refine(inner), environment);
    cx.finish(ui, root);
    root
}
#[cfg_attr(debug_assertions, inline(never))]
fn mount_overlay(
    ui: &mut Ui,
    parent: NodeId,
    kind: Kind,
    mut view: View,
    environment: Environment,
) -> NodeId {
    match kind {
        Kind::Modal {
            label,
            open,
            anchor,
        } => mount_portal(
            ui,
            parent,
            label,
            open,
            anchor,
            view,
            environment,
            false,
            false,
        ),
        Kind::Menu {
            label,
            open,
            anchor,
        } => mount_portal(
            ui,
            parent,
            label,
            open,
            Some(anchor),
            view,
            environment,
            true,
            false,
        ),
        Kind::Submenu { label, open } => {
            dim_when_disabled(&mut view);
            // The app's own trigger row (a menu item with content), or the
            // label with a chevron.
            let trigger = view.rare.as_mut().and_then(|r| r.portal.as_mut());
            let mut anchor = match trigger.and_then(|p| p.trigger.take()) {
                Some(trigger) => trigger,
                // Its label followed (`accessible_label_when`) by the row too.
                None => match followed_label(view.rare.as_deref_mut()) {
                    Some(followed) => {
                        menu_item(label.clone()).accessible_label_when(move || followed.get())
                    }
                    None => menu_item(label.clone()),
                },
            };
            anchor.rare_mut().menu_trigger = Some(open.clone());
            if let Some(id) = view.rare.as_mut().and_then(|r| r.trigger_id.take()) {
                anchor.id = Some(id);
            }
            mount_portal(
                ui,
                parent,
                label,
                open,
                Some(Box::new(anchor)),
                view,
                environment,
                true,
                true,
            )
        }
        _ => unreachable!("an overlay kind"),
    }
}
#[allow(clippy::too_many_arguments)]
fn mount_portal(
    ui: &mut Ui,
    parent: NodeId,
    label: String,
    open: Signal<bool>,
    anchor: Option<Box<View>>,
    mut view: View,
    environment: Environment,
    is_menu: bool,
    is_submenu: bool,
) -> NodeId {
    let anchored = anchor.is_some();
    let owner = ui.container(
        parent,
        Layout::Overlay,
        crate::scene::Style {
            absolute: !anchored,
            align: if is_submenu {
                crate::scene::Align::Stretch
            } else {
                crate::scene::Align::Start
            },
            justify: if is_submenu {
                crate::scene::Justify::Stretch
            } else {
                crate::scene::Justify::Start
            },
            width: (!anchored).then_some(0.),
            height: (!anchored).then_some(0.),
            ..Default::default()
        },
    );
    if is_submenu && let Some(mut disabled) = view.disabled.take() {
        let weak = ui.downgrade();
        ui.bind(owner, move || {
            let disabled = disabled();
            if let Some(mut ui) = weak.upgrade() {
                ui.set_disabled(owner, disabled);
            }
        });
    }
    // A button with no click handler of its own.
    let bare = anchor
        .as_ref()
        .is_some_and(|a| a.click.is_none() && matches!(a.kind, Kind::Button));
    let anchor = anchor.map(|anchor| mount(ui, owner, *anchor, environment.clone()));
    if let Some(anchor) = anchor {
        ui.listen_popup_soon(anchor);
    }
    // A bare button opening a menu (no handlers of its own) is a menu
    // button, as WAI-ARIA's: a press, Enter, Space or Down opens it, even
    // before its panel is first built (`lazy`).
    // Whether its panel is built: until then (`lazy`) nothing else opens
    // it from a press.
    let built = Rc::new(Cell::new(false));
    if let Some(anchor) = anchor.filter(|_| is_menu && !is_submenu && bare) {
        {
            let (open, built) = (open.clone(), built.clone());
            let binding = ui.input.listen(anchor, move |cx| {
                if cx.phase == EventPhase::Capture || open.with_untracked(|o| *o) {
                    return;
                }
                let opens = match cx.event {
                    InputEvent::PointerDown {
                        button: crate::input::PointerButton::Primary,
                        ..
                    } => !built.get(),
                    InputEvent::Activate
                    | InputEvent::KeyDown {
                        key: crate::input::Key::ArrowDown,
                        ..
                    } => true,
                    _ => false,
                };
                if opens {
                    cx.prevent_default();
                    open.set(true);
                }
            });
            ui.retain(anchor, binding);
        }
    }
    let options = view.rare.as_ref().and_then(|r| r.portal.as_ref());
    if !options.is_some_and(|p| p.lazy) {
        built.set(true);
        mount_portal_panel(
            ui,
            owner,
            anchor,
            label,
            open,
            view,
            environment,
            is_menu,
            is_submenu,
        );
        return owner;
    }
    // Lazy (`View::lazy`): built the first time it opens, and kept.
    let mut pending = Some((label, open.clone(), view, environment));
    let weak = ui.downgrade();
    let runtime = ui.runtime.clone();
    ui.bind(owner, move || {
        if !open.get() || pending.is_none() {
            return;
        }
        let Some(mut ui) = weak.upgrade() else { return };
        if !ui.scene.borrow().contains(owner) {
            return;
        }
        let (label, open, view, environment) = pending.take().expect("checked above");
        built.set(true);
        runtime.untracked(|| {
            mount_portal_panel(
                &mut ui,
                owner,
                anchor,
                label,
                open,
                view,
                environment,
                is_menu,
                is_submenu,
            )
        });
    });
    owner
}
/// The rest of `mount_portal`, when it first opens.
#[allow(clippy::too_many_arguments)]
fn mount_portal_panel(
    ui: &mut Ui,
    owner: NodeId,
    anchor: Option<NodeId>,
    label: String,
    open: Signal<bool>,
    mut view: View,
    mut environment: Environment,
    is_menu: bool,
    is_submenu: bool,
) {
    let anchored = anchor.is_some();
    let panel_hidden = view.rare.as_ref().is_some_and(|r| r.panel_hidden);
    if let Some(anchor) =
        anchor.filter(|_| view.rare.as_ref().is_some_and(|r| r.match_anchor_width))
    {
        let bounds = ui.observe_bounds(anchor);
        let mut own = view.reactive_style.take();
        view.reactive_style = Some(Box::new(move |out: &mut Option<Styles>| {
            let mut styles = None;
            if let Some(f) = own.as_mut() {
                f(&mut styles);
            }
            *out = Some(styles.unwrap_or_else(Styles::new).min_w(bounds.get().width));
        }));
    }
    let portal_parent = environment
        .portal
        .as_ref()
        .map_or(ui.root(), |(node, _)| *node);
    let parent_visible = environment
        .portal
        .as_ref()
        .map(|(_, visible)| visible.clone());
    let portal = crate::compose_modal::create(ui, owner, portal_parent);
    if !anchored {
        // The theme's backdrop, following it.
        let (scene, theme, overlay) = (ui.scene.clone(), ui.theme_scope(), portal.overlay);
        ui.bind(overlay, move || {
            let fill = theme.color(tok::BACKDROP);
            scene.borrow_mut().set_kind(
                overlay,
                NodeKind::Panel {
                    layout: Layout::Overlay,
                    quad: crate::scene::QuadStyle {
                        fill,
                        ..Default::default()
                    },
                },
            );
        });
    }
    environment.portal = Some((portal.overlay, portal.visible.clone()));
    let menu_context = is_menu.then(|| {
        crate::compose_menu::MenuContext::new(
            open.clone(),
            environment.menu.clone(),
            anchor,
            is_submenu,
        )
    });
    if let Some(menu) = &menu_context {
        environment.menu = Some(menu.clone());
    }

    let options = view.rare.as_ref().and_then(|r| r.portal.as_ref());
    let is_layer = options.is_some_and(|p| p.layer);
    // At an editor's caret: placed there, and focus left in the editor, as
    // a layer leaves it.
    let at_caret = options.is_some_and(|p| p.at_caret);
    let keeps_focus = is_layer || at_caret;
    // A layer's panel is only what it holds; a dialog's or menu's is a card.
    let mut defaults = if is_layer {
        Styles::new()
    } else {
        Styles::new()
            .w(if anchored { 240. } else { 360. })
            .p(16.)
            .bg(if is_menu {
                tok::MENU_BG
            } else if anchored {
                tok::POPOVER_BG
            } else {
                tok::SURFACE_OVERLAY
            })
            .rounded(tok::RADIUS_MD)
            .overflow_hidden()
    };
    if is_menu {
        defaults = defaults.items_stretch();
    }
    defaults.merge(&view.styles);
    view.styles = defaults;
    let set_max = (view.styles.rare().max_width, view.styles.rare().max_height);
    let (has_max_width, has_max_height) = (set_max.0.is_some(), set_max.1.is_some());
    let viewport = ui.observe_bounds(ui.root());
    // Anchored, it may reach past the window, to the screen's edges, and be
    // cut to the room there by the platform (`Ui::set_popup_limit`).
    let area = anchor.map(|anchor| (ui.popup_screens(), ui.observe_bounds(anchor)));
    let limit = anchor.map(|_| (ui.popup_limits(), portal.position));
    let mut dynamic = view.reactive_style.take();
    view.reactive_style = Some(Box::new(move |out: &mut Option<Styles>| {
        let mut styles = None;
        if let Some(compute) = dynamic.as_mut() {
            compute(&mut styles);
        }
        let mut styles = styles.unwrap_or_else(Styles::new);
        let bounds = area
            .as_ref()
            .and_then(|(screens, anchor)| {
                let anchor = anchor.get();
                screens.with(|screens| crate::compose_popover::screen_of(screens, anchor))
            })
            .unwrap_or_else(|| viewport.get());
        let margin = if anchored { 8. } else { 32. };
        if !has_max_width && styles.rare().max_width.is_none() {
            styles.rare_mut().max_width = Some((bounds.width - margin).max(0.));
        }
        if !has_max_height && styles.rare().max_height.is_none() {
            styles.rare_mut().max_height = Some((bounds.height - margin).max(0.));
        }
        if let Some((width, height)) = limit.as_ref().and_then(|(limits, root)| {
            limits.with(|limits| {
                limits
                    .iter()
                    .find(|(limited, _)| limited == root)
                    .map(|(_, size)| *size)
            })
        }) {
            let rare = styles.rare_mut();
            let cut = |max: Option<f32>, to: f32| Some(max.map_or(to, |max| max.min(to)));
            rare.max_width = cut(rare.max_width.or(set_max.0), width);
            rare.max_height = cut(rare.max_height.or(set_max.1), height);
        }
        *out = Some(styles);
    }));
    // A submenu leaves the pointer to its parent menu (moving over its other
    // items closes it); the parent's backdrop dismisses both.
    let mut rare = view.rare.as_mut();
    let dismiss_backdrop = rare.as_mut().and_then(|r| r.dismiss_backdrop.take());
    let dismiss_backdrop = dismiss_backdrop.unwrap_or(!is_submenu);
    let (placement, modal_placement, autofocus, over, arrow) = rare
        .and_then(|r| r.portal.take())
        .map_or((None, None, None, None, None), |p| {
            (p.placement, p.modal_placement, p.autofocus, p.over, p.arrow)
        });
    // The arrow is drawn as the panel is: a copy of its styles.
    let arrow = arrow.filter(|_| anchored && !is_menu).map(|size| {
        let mut styles = Styles::new();
        styles.merge(&view.styles);
        (size, styles)
    });
    let placement = placement.unwrap_or_default();
    let panel_focusable = view.focusable.unwrap_or(true);
    let layout_target = is_menu.then(|| ui.signal(None));
    if layout_target.is_some() || view.rare.is_some() {
        view.rare_mut().layout_target = layout_target.clone();
    }
    let menu_children = if is_menu {
        std::mem::take(&mut view.children)
    } else {
        Vec::new()
    };
    let panel = mount_element(
        ui,
        portal.position,
        Kind::Container(Layout::Column),
        view,
        environment.clone(),
    );
    if let Some(menu) = menu_context.clone() {
        menu.panel.set(Some(panel));
        let viewport = ui.container(
            panel,
            Layout::Overlay,
            crate::scene::Style {
                clip: true,
                flex_shrink: 1.,
                flex_grow: 1.,
                ..Default::default()
            },
        );
        let content = ui.container(
            viewport,
            Layout::Column,
            crate::scene::Style {
                flex_shrink: 0.,
                align: crate::scene::Align::Stretch,
                ..Default::default()
            },
        );
        layout_target.as_ref().unwrap().set(Some(content));
        for child in menu_children {
            mount(ui, content, child, environment.clone());
        }
        crate::compose_menu_scroll::mount(ui, panel, viewport, content);
        let trigger = anchor.expect("menu anchor");
        if !is_submenu {
            crate::compose_menu::mount_press(ui, trigger, portal.overlay, panel, open.clone());
        }
        crate::compose_menu::mount(ui, panel, menu, trigger, portal.overlay, dismiss_backdrop);
    }
    let mut over_focus = None;
    if let Some(anchor) = anchor {
        if is_submenu {
            crate::compose_popover::mount_submenu(
                ui,
                owner,
                anchor,
                portal.overlay,
                portal.position,
                panel,
            );
        } else {
            // `open_over`'s item, among the menu's.
            let over_item = over.map(|id| {
                let (registry, weak) = (environment.registry.clone(), ui.downgrade());
                Rc::new(move || {
                    let id = id();
                    let ui = weak.upgrade()?;
                    let scene = ui.scene.borrow();
                    let registry = registry.borrow();
                    registry
                        .get(&id)?
                        .iter()
                        .copied()
                        .find(|node| scene.contains(*node) && scene.is_within(*node, panel))
                }) as Rc<dyn Fn() -> Option<NodeId>>
            });
            over_focus = over_item.clone();
            let over =
                over_item.map(|item| Box::new(move || item()) as Box<dyn Fn() -> Option<NodeId>>);
            // The arrow: a square drawn as the panel is, turned on its corner,
            // in a box showing the half past the panel's edge (placed with
            // the panel, `compose_popover::place_arrow`).
            let arrow = arrow.map(|(size, styles)| {
                let clip = ui.container(
                    portal.position,
                    Layout::Overlay,
                    crate::scene::Style {
                        absolute: true,
                        clip: true,
                        width: Some(2. * size),
                        height: Some(size),
                        ..Default::default()
                    },
                );
                // Its box is the popover's, though the square paints nothing
                // where the panel is clear (a macOS popover's material shows
                // there, `Ui::set_popup_screens`): a popup window holds it.
                ui.scene.borrow_mut().set_platform_painted(clip, true);
                let side = size * std::f32::consts::SQRT_2;
                let mut square = div();
                square.styles = styles;
                let square = square
                    .absolute()
                    .w(side)
                    .h(side)
                    .min_w(side)
                    .min_h(side)
                    .max_w(side)
                    .max_h(side)
                    .p(0.)
                    .rounded(0.)
                    .rotate(std::f32::consts::FRAC_PI_4);
                let diamond = mount(ui, clip, square, environment.clone());
                crate::compose_popover::Arrow {
                    clip,
                    diamond,
                    size,
                }
            });
            // At the caret: placed against the caret of the editor in the
            // trigger (its box, where it has none).
            let placed_at = match at_caret {
                true => ui
                    .editor_within(anchor)
                    .map_or(anchor, |editor| editor.caret),
                false => anchor,
            };
            crate::compose_popover::mount(
                ui,
                owner,
                placed_at,
                portal.overlay,
                portal.position,
                panel,
                placement,
                over,
                arrow,
            );
        }
    } else {
        match modal_placement {
            Some(at) => crate::compose_modal::place(ui, owner, portal.position, at),
            None => crate::compose_modal::center(ui, owner, portal.position),
        }
    }
    let open_signal = open.clone();
    let popup_open = open.clone();
    let unnamed = label.is_empty();
    crate::compose_modal::activate(
        ui,
        owner,
        &portal,
        panel,
        label,
        open,
        parent_visible,
        dismiss_backdrop,
        if is_menu { Role::Menu } else { Role::Dialog },
        menu_context
            .as_ref()
            .and_then(|menu| menu.parent.as_ref())
            .and_then(|parent| parent.panel.get()),
        if is_submenu { anchor } else { None },
        panel_focusable,
        is_submenu,
        autofocus.unwrap_or(true),
        keeps_focus,
    );
    // An unnamed popover or menu is named by its trigger, as a browser's
    // popover target is (`aria-labelledby`).
    if let Some(anchor) = anchor.filter(|_| unnamed && !is_submenu) {
        ui.semantics.borrow_mut().set_labelled_by(panel, anchor);
    }
    // A popover closes on a press outside it (in the window, in a parent
    // popover's panel), and the press goes on to what it lands on.
    if let Some(anchor) = anchor.filter(|_| dismiss_backdrop && !is_menu && !keeps_focus) {
        crate::compose_popover::mount_dismissing(
            ui,
            owner,
            anchor,
            panel,
            portal.overlay,
            open_signal.clone(),
            portal.visible.clone(),
        );
    }
    // A layer (a toast, a tooltip) traps nothing, so it isn't modal; a
    // popover traps focus as a dialog does, so it is.
    ui.semantics.borrow_mut().update(panel, |node| {
        node.modal = node.modal && !keeps_focus;
        node.hidden |= panel_hidden;
    });
    // A popover's trigger says it opens a dialog, and whether it shows (a
    // menu's says so of its menu; a tooltip's says nothing).
    if let Some(anchor) = anchor.filter(|_| !is_menu && !panel_hidden) {
        let semantics = ui.semantics.clone();
        ui.bind(panel, move || {
            let expanded = popup_open.get();
            semantics.borrow_mut().update(anchor, |node| {
                node.has_popup = Some(crate::semantics::PopupKind::Dialog);
                node.expanded = Some(expanded);
            });
        });
    }
    // A pop-up button's menu opens with its chosen item (`open_over`)
    // highlighted, rather than its first.
    if let Some(item) = over_focus {
        let (visible, weak) = (portal.visible.clone(), ui.downgrade());
        ui.bind(owner, move || {
            if visible.get()
                && let Some(ui) = weak.upgrade()
                && let Some(item) = ui.runtime.untracked(|| item())
            {
                ui.input.focus(&ui.scene, Some(item));
            }
        });
    }
    // Open, where the host shows popups past the window
    // (`Ui::set_popup_screens`), an anchored popup is a popup root: drawn in
    // a window of its own (`Scene::set_popup`). Closed, it is none, so a
    // list's hundred row menus cost the host nothing.
    if anchored {
        let (screens, visible, position) =
            (ui.popup_screens(), portal.visible.clone(), portal.position);
        let weak = ui.downgrade();
        ui.bind(owner, move || {
            let popup = visible.get() && screens.with(|screens| !screens.is_empty());
            if let Some(ui) = weak.upgrade() {
                let mut scene = ui.scene.borrow_mut();
                scene.set_popup(position, popup);
                // Its trigger, which a platform placing it flips it around.
                if popup && let Some(anchor) = anchor {
                    scene.set_popup_anchor(position, anchor);
                }
            }
        });
        ui.register_popup(portal.position, open_signal);
    }
    // A submenu's layer lets the pointer through to its parent menu.
    if is_submenu {
        let mut options = ui.input.options(portal.overlay).unwrap_or_default();
        options.pass_through = true;
        ui.input.set_options(portal.overlay, options);
    }
}
/// The built-in widgets' default styles, built once per thread and cloned
/// for each widget: their theme token bindings are shared, not allocated
/// for every button.
struct WidgetStyles {
    /// Focus on a control without a border: an inset ring, taking no room,
    /// so focusing moves nothing; with one, its border recoloured.
    focus_ring: Styles,
    focus_recolor: Styles,
    menu_focus_recolor: Styles,
    menu_item: Styles,
    menu_hover: Styles,
    menu_focus: Styles,
    button: Styles,
    button_hover: Styles,
    button_pressed: Styles,
    progress: Styles,
    slider: Styles,
    editor: Styles,
}
impl WidgetStyles {
    fn new() -> Self {
        Self {
            focus_ring: Styles::new()
                .border(1.)
                .border_color(tok::FOCUS_RING)
                .border_over(true),
            focus_recolor: Styles::new().border_color(tok::FOCUS_RING),
            menu_focus_recolor: Styles::new()
                .bg(tok::MENU_HOVER)
                .border_color(tok::FOCUS_RING),
            // On the menu's panel: no fill of its own, so a translucent
            // panel (AppKit's material behind it) shows through.
            menu_item: Styles::new().px(12.).py(8.).items_center(),
            menu_hover: Styles::new().bg(tok::MENU_HOVER),
            menu_focus: Styles::new()
                .bg(tok::MENU_HOVER)
                .border(1.)
                .border_color(tok::FOCUS_RING)
                .border_over(true),
            button: Styles::new()
                .px(12.)
                .py(8.)
                .gap(6.)
                .items_center()
                .bg(tok::BUTTON_BG)
                .rounded(tok::RADIUS_MD),
            button_hover: Styles::new().bg(tok::BUTTON_HOVER),
            button_pressed: Styles::new().bg(tok::BUTTON_PRESSED),
            progress: Styles::new().w(240.).h(8.).bg(tok::INPUT_BG),
            slider: Styles::new()
                .w(240.)
                .h(tok::CONTROL_HEIGHT)
                .px(8.)
                .py(6.)
                .bg(tok::INPUT_BG)
                .rounded(tok::RADIUS_MD),
            // Outlined as a browser's fields are, so a field shows on any
            // surface (the input colour can be the panel's); drawn over the
            // content, so it takes no room.
            editor: Styles::new()
                .bg(tok::INPUT_BG)
                .rounded(tok::RADIUS_MD)
                .border(1.)
                .border_color(tok::BORDER)
                .border_over(true),
        }
    }
}
/// Dimmed while disabled, as native controls are, unless the view says
/// how it looks then (`disabled_style`).
fn dim_when_disabled(view: &mut View) {
    if view.variants.disabled.is_none() {
        view.variants.disabled = Some(Box::new(Styles::new().opacity(0.45)));
    }
}
fn widget_style(pick: impl FnOnce(&WidgetStyles) -> &Styles) -> Styles {
    thread_local! { static STYLES: WidgetStyles = WidgetStyles::new(); }
    STYLES.with(|styles| pick(styles).clone())
}
fn mount_element(
    ui: &mut Ui,
    parent: NodeId,
    kind: Kind,
    mut view: View,
    environment: Environment,
) -> NodeId {
    let _theme = ui.enter_theme(&environment.theme);
    // Decorated/padded text gets an actual layout box. Plain text stays a single
    // node, keeping common reactive labels small. Dynamic/state styles may add
    // decoration later, so reserve that box at mount rather than remount on hover.
    if matches!(kind, Kind::Text(_) | Kind::BoundText(_)) {
        assert!(
            view.children.is_empty(),
            "text views cannot contain child views"
        );
        let boxed = view.styles.padding.is_some()
            || view.styles.background.is_some()
            || view.styles.radius.is_some()
            || view.styles.border_width.is_some()
            || view.styles.rare().shadow.is_some()
            || view.reactive_style.is_some()
            || view.variants.hover.is_some()
            || view.variants.active.is_some()
            || view.variants.focus.is_some()
            || view.variants.disabled.is_some()
            || view.variants.reactive.is_some();
        if boxed {
            let label = View::new(kind);
            view.children.push(label);
            // The label spans the box, so its lines align (`text_align`)
            // and truncate at the box's edges, not its own.
            let mut defaults = Styles::new().items_stretch();
            defaults.merge(&view.styles);
            view.styles = defaults;
            return mount_element(
                ui,
                parent,
                Kind::Container(Layout::Column),
                view,
                environment,
            );
        }
    }
    // What few views set, out of its box for the rest of mounting.
    let mut rare = view.rare.take();
    assert!(
        rare.as_ref().is_none_or(|r| r.trigger_id.is_none()),
        "trigger_id is only supported on submenu views"
    );
    assert!(
        rare.as_ref().is_none_or(|r| r.dismiss_backdrop.is_none()),
        "dismiss_on_backdrop is only supported on modal and popover views"
    );
    assert!(
        rare.as_ref()
            .is_none_or(|r| r.keyboard_navigation.is_none())
            || matches!(kind, Kind::VirtualList(_)),
        "keyboard_navigation is only supported on virtual_list views"
    );
    let keyboard_navigation = rare
        .as_ref()
        .and_then(|r| r.keyboard_navigation)
        .unwrap_or(false);
    assert!(
        rare.as_ref().is_none_or(|r| r.scrollbar.is_none())
            || matches!(kind, Kind::Scroll { .. } | Kind::VirtualList(_)),
        "scrollbar is only supported on scroll, scroll_x, and virtual_list views"
    );
    let scrollbar = rare.as_ref().and_then(|r| r.scrollbar).unwrap_or(false);
    let follow_end = rare.as_ref().and_then(|r| r.follow_end).unwrap_or(false);
    let mut opened = open_element(
        ui,
        parent,
        kind,
        view,
        rare.as_deref_mut(),
        environment,
        keyboard_navigation,
        scrollbar,
        follow_end,
    );
    ui.open_element(opened.root);
    for child in std::mem::take(&mut opened.children) {
        mount(ui, opened.child_parent, child, opened.environment.clone());
    }
    close_element(ui, opened)
}
/// An element mounted up to its children: what the rest of its mounting
/// needs once they are in (`mount_element`).
struct Opened {
    children: Vec<View>,
    conditional: Vec<RegionBuilder>,
    row_text: Option<crate::selection::RowText>,
    pointer_events: Option<Box<dyn FnMut() -> bool>>,
    accessible_label: Option<String>,
    accessibility: Option<Box<Accessibility>>,
    mounted: Vec<Box<dyn FnOnce(NodeHandle)>>,
    /// Whether the view said if it takes focus.
    focusable_set: bool,
    environment: Environment,
    root: NodeId,
    child_parent: NodeId,
    submenu_indicator: bool,
    scroll_offset: Option<(Signal<f32>, bool, bool)>,
    scroll_nodes: Option<(NodeId, NodeId)>,
    scrollbar: bool,
    follow_end: bool,
    compute: Option<Box<dyn FnMut() -> String>>,
    region: Option<RegionBuilder>,
    focusable: bool,
    /// A canvas: named, it is a picture (a chart, by what it shows), read
    /// as an image, as an `img` is, not a group.
    pictured: bool,
}
/// The element's node, before its children. Apart from `mount_element`,
/// whose frame stays live while they mount: in a debug build each of this
/// function's values takes stack of its own, and trees nest deep.
#[cfg_attr(debug_assertions, inline(never))]
#[cfg_attr(not(debug_assertions), inline(always))]
#[allow(clippy::too_many_arguments)]
fn open_element(
    ui: &mut Ui,
    parent: NodeId,
    kind: Kind,
    mut view: View,
    mut rare: Option<&mut Rare>,
    mut environment: Environment,
    keyboard_navigation: bool,
    scrollbar: bool,
    follow_end: bool,
) -> Opened {
    let is_scroll = matches!(kind, Kind::Scroll { .. });
    let mut scroll_offset = None;
    let is_image = matches!(kind, Kind::Image { .. });
    #[cfg(target_os = "macos")]
    let is_image = is_image || matches!(kind, Kind::NativeSurface { .. });
    #[cfg(target_os = "macos")]
    let mut native_source = None;
    let mut image_source = None;
    let mut image_intrinsic = None;
    let is_menu_item = matches!(kind, Kind::MenuItem(_));
    let is_submenu_trigger =
        is_menu_item && rare.as_deref().is_some_and(|r| r.menu_trigger.is_some());
    // A trigger drawn by the app (`View::trigger`) shows its own chevron.
    let submenu_indicator = is_submenu_trigger && view.children.is_empty();
    // Outside a menu (a popover's list) an item is a plain action.
    let item_menu = is_menu_item.then(|| environment.menu.clone()).flatten();
    let is_editor = matches!(kind, Kind::Editor { .. });
    assert!(
        rare.as_deref().is_none_or(|r| r.read_only.is_none()) || is_editor,
        "read_only is only supported on editor views"
    );
    assert!(
        rare.as_deref().is_none_or(|r| r.spellcheck.is_none()) || is_editor,
        "spellcheck is only supported on editor views"
    );
    assert!(
        rare.as_deref().is_none_or(|r| r.input_mode.is_none()) || is_editor,
        "input_mode is only supported on editor views"
    );
    assert!(
        (rare.as_deref().is_none_or(|r| r.placeholder.is_none())
            && rare
                .as_deref()
                .is_none_or(|r| r.placeholder_color.is_none()))
            || is_editor,
        "placeholder is only supported on editor views"
    );
    assert!(
        rare.as_deref()
            .is_none_or(|r| r.caret_color.is_none() && r.caret_width.is_none())
            || is_editor,
        "caret_color and caret_width are only supported on editor views"
    );
    assert!(
        rare.as_deref().is_none_or(|r| r.select_on_focus.is_none()) || is_editor,
        "select_on_focus is only supported on editor views"
    );
    assert!(
        rare.as_deref().is_none_or(|r| r.line_numbers.is_none()) || is_editor,
        "line_numbers is only supported on editor views"
    );
    assert!(
        rare.as_deref().is_none_or(|r| r.code.is_none()) || is_editor,
        "highlighter, decorations, current_line and tab_indent are only supported on editor views"
    );
    if is_editor {
        assert!(
            view.children.is_empty(),
            "editor views cannot contain child views"
        );
    }
    let is_slider = matches!(kind, Kind::Slider { .. });
    let is_progress = matches!(kind, Kind::Progress { .. });
    let mut slider_state = None;
    let mut progress_state = None;
    let is_checkbox = matches!(kind, Kind::Checkbox { .. });
    let interactive = matches!(
        kind,
        Kind::Button | Kind::Checkbox { .. } | Kind::MenuItem(_)
    ) || view.click.is_some();
    let mut checkbox_value = None;
    let mut editor = None;
    let mut region = None;
    let mut compute = None;
    let mut rich_compute = None;
    let mut rich_paragraph = None;
    let mut canvas_compute = None;
    let mut svg_compute = None;
    let root = match kind {
        Kind::Canvas(draw) => {
            assert!(view.children.is_empty(), "canvas views own their drawing");
            canvas_compute = Some(draw);
            let root = ui.container(parent, Layout::Overlay, Default::default());
            ui.scene.borrow_mut().set_replaced(root, true);
            root
        }
        Kind::Svg { label, source } => {
            assert!(view.children.is_empty(), "SVG views own their artwork");
            let root = ui.container(parent, Layout::Overlay, Default::default());
            ui.scene.borrow_mut().set_replaced(root, true);
            ui.semantics
                .borrow_mut()
                .set(root, SemanticNode::new(Role::Image, label));
            svg_compute = Some(source);
            root
        }
        #[cfg(target_os = "macos")]
        Kind::NativeSurface { label, mut source } => {
            assert!(view.children.is_empty(), "native surfaces own their pixels");
            let root = ui.container(parent, Layout::Overlay, Default::default());
            ui.scene.borrow_mut().set_replaced(root, true);
            ui.semantics
                .borrow_mut()
                .set(root, SemanticNode::new(Role::Image, label));
            let intrinsic = ui.signal((0., 0.));
            let intrinsic_write = intrinsic.clone();
            let data = ui.signal(None);
            let write = data.clone();
            ui.bind(root, move || {
                let frame = source();
                intrinsic_write.set((frame.width() as f32, frame.height() as f32));
                write.set(Some(frame));
            });
            native_source = Some(data);
            image_intrinsic = Some(intrinsic);
            root
        }
        Kind::Image { label, mut source } => {
            assert!(view.children.is_empty(), "image views own their pixels");
            let root = ui.container(parent, Layout::Overlay, Default::default());
            ui.scene.borrow_mut().set_replaced(root, true);
            ui.semantics
                .borrow_mut()
                .set(root, SemanticNode::new(Role::Image, label));
            let intrinsic = ui.signal((0., 0.));
            let intrinsic_write = intrinsic.clone();
            let data = ui.signal(None);
            let data_write = data.clone();
            ui.bind(root, move || {
                let next = source();
                intrinsic_write.set((next.width() as f32, next.height() as f32));
                data_write.set(Some(next));
            });
            image_source = Some(data);
            image_intrinsic = Some(intrinsic);
            root
        }
        Kind::RichText(binding) => {
            assert!(
                view.children.is_empty(),
                "rich text uses inline TextSpan children"
            );
            rich_compute = Some(binding);
            let root = ui.container(parent, Layout::Overlay, Default::default());
            let paragraph = ui.label(root, "", Default::default());
            rich_paragraph = Some(paragraph);
            ui.scene.borrow_mut().set_kind(
                paragraph,
                NodeKind::RichText {
                    text: std::sync::Arc::new(
                        crate::rich_text::RichText::new("", Vec::new()).unwrap(),
                    ),
                },
            );
            root
        }
        Kind::Text(text) => ui.label(parent, text, Default::default()),
        Kind::BoundText(binding) => {
            compute = Some(binding);
            ui.label(parent, "", Default::default())
        }
        Kind::Container(layout) => ui.container(parent, layout, Default::default()),
        Kind::MenuItem(label) => {
            dim_when_disabled(&mut view);
            // An item draws its label, unless it brings content of its own
            // (an icon, the label, a shortcut): the label stays its
            // accessible name and what typing finds it by.
            let mut defaults = widget_style(|w| &w.menu_item);
            defaults.merge(&view.styles);
            view.styles = defaults;
            if view.variants.hover.is_none() {
                view.variants.defaults[0] = Some(|| widget_style(|w| &w.menu_hover));
            }
            if view.variants.focus.is_none() {
                view.variants.defaults[2] = Some(menu_focus(&view.styles));
            }
            let root = ui.container(parent, Layout::Row, Default::default());
            let mut semantic = SemanticNode::new(Role::MenuItem, label.clone());
            let check = rare.as_deref().and_then(|r| r.menu_check.clone());
            let shortcut = rare.as_deref().and_then(|r| r.shortcut.clone());
            semantic.checked = check.as_ref().map(|(checked, _)| checked());
            semantic.radio = check.as_ref().is_some_and(|(_, radio)| *radio);
            semantic.shortcut = shortcut.clone();
            ui.semantics.borrow_mut().set(root, semantic);
            if let Some((checked, _)) = &check {
                let (tree, checked) = (ui.semantics.clone(), checked.clone());
                ui.bind(root, move || {
                    let now = checked();
                    tree.borrow_mut().update(root, |n| n.checked = Some(now));
                });
            }
            if view.children.is_empty() {
                // The check (or a radio's dot) in a gutter of its own.
                if let Some((checked, radio)) = check {
                    let mark = if radio { "•" } else { "✓" };
                    view.children.push(
                        text_signal(move || {
                            if checked() {
                                mark.into()
                            } else {
                                String::new()
                            }
                        })
                        .w(16.)
                        .shrink_0()
                        .accessible_hidden(),
                    );
                }
                let drawn = menu_text(label, rare.as_deref_mut());
                if is_submenu_trigger || shortcut.is_some() {
                    view.children.push(drawn.flex_grow(1.));
                } else {
                    view.children.push(drawn);
                }
                if let Some(keys) = shortcut {
                    view.children.push(
                        text(keys)
                            .ml(24.)
                            .text_color(tok::TEXT_TERTIARY)
                            .accessible_hidden(),
                    );
                }
            }
            root
        }
        Kind::Button => {
            dim_when_disabled(&mut view);
            let mut defaults = widget_style(|w| &w.button);
            defaults.merge(&view.styles);
            view.styles = defaults;
            if view.variants.hover.is_none() {
                view.variants.defaults[0] = Some(|| widget_style(|w| &w.button_hover));
            }
            if view.variants.active.is_none() {
                view.variants.defaults[1] = Some(|| widget_style(|w| &w.button_pressed));
            }
            if view.variants.focus.is_none() {
                view.variants.defaults[2] = Some(focus_ring(&view.styles));
            }
            ui.container(parent, Layout::Row, Default::default())
        }
        Kind::Checkbox {
            label,
            value,
            mixed,
        } => {
            assert!(
                view.children.is_empty(),
                "checkbox views own their indicator and label"
            );
            let mut defaults = Styles::new().gap(6.).items_center().px(4.).py(4.);
            defaults.merge(&view.styles);
            view.styles = defaults;
            if view.variants.hover.is_none() {
                view.variants.defaults[0] = Some(|| widget_style(|w| &w.button_hover));
            }
            if view.variants.focus.is_none() {
                view.variants.defaults[2] = Some(focus_ring(&view.styles));
            }
            dim_when_disabled(&mut view);
            let root = ui.container(parent, Layout::Row, Default::default());
            let mut semantics = SemanticNode::new(Role::CheckBox, label.clone());
            semantics.checked = Some(value.get());
            ui.semantics.borrow_mut().set(root, semantics);
            let (checked, is_mixed) = (value.clone(), mixed.clone());
            let semantic_tree = ui.semantics.clone();
            ui.bind(root, move || {
                let checked = checked.get();
                let mixed = is_mixed.as_ref().is_some_and(|m| m.get());
                semantic_tree.borrow_mut().update(root, |node| {
                    node.checked = Some(checked);
                    node.mixed = mixed;
                });
            });
            // The box, drawn as the platform draws one.
            view.children.push(crate::compose_checkbox::indicator(
                value.clone(),
                mixed,
                ui.theme_scope(),
                environment.services.get::<TaskRunner>(),
            ));
            view.children.push(text(label));
            checkbox_value = Some(value);
            root
        }
        Kind::Progress { label, value } => {
            dim_when_disabled(&mut view);
            assert!(view.children.is_empty(), "progress views own their fill");
            let mut defaults = widget_style(|w| &w.progress);
            defaults.merge(&view.styles);
            view.styles = defaults;
            let root = ui.container(parent, Layout::Overlay, Default::default());
            let mut semantic = SemanticNode::new(Role::Progress, label);
            semantic.min = Some(0.);
            semantic.max = Some(1.);
            ui.semantics.borrow_mut().set(root, semantic);
            progress_state = Some(value);
            root
        }
        Kind::Slider {
            label,
            value,
            range,
        } => {
            dim_when_disabled(&mut view);
            assert!(
                view.children.is_empty(),
                "slider views own their rail and thumb"
            );
            let mut defaults = widget_style(|w| &w.slider);
            defaults.merge(&view.styles);
            view.styles = defaults;
            if view.variants.focus.is_none() {
                view.variants.defaults[2] = Some(focus_ring(&view.styles));
            }
            let root = ui.container(parent, Layout::Overlay, Default::default());
            let mut semantic = SemanticNode::new(Role::Slider, label);
            semantic.min = Some(*range.start() as f64);
            semantic.max = Some(*range.end() as f64);
            ui.semantics.borrow_mut().set(root, semantic);
            let step = rare.as_deref().and_then(|r| r.slider_step);
            slider_state = Some((value, range, step));
            root
        }
        Kind::Editor {
            label,
            value,
            multiline,
        } => {
            dim_when_disabled(&mut view);
            let mut defaults = widget_style(|w| &w.editor);
            // The default outline is drawn over; a border the app gives
            // takes room, as CSS's does, unless it says otherwise.
            let own_border =
                view.styles.border_width.is_some() && view.styles.rare().border_over.is_none();
            defaults.merge(&view.styles);
            if own_border {
                defaults = defaults.border_over(false);
            }
            view.styles = defaults;
            if view.variants.focus.is_none() {
                view.variants.defaults[2] = Some(focus_ring(&view.styles));
            }
            let handle = ui.text_input_styled(parent, label, value, multiline);
            let root = handle.node;
            editor = Some(handle);
            root
        }
        Kind::Scroll { offset, horizontal } => {
            let mut defaults = Styles::new().overflow_hidden();
            defaults.merge(&view.styles);
            view.styles = defaults;
            // 320 by 240 unless something sizes it: a parent stretching it
            // across, `grow`, or its own `w`/`h` (which then hold).
            let root = ui.container(parent, Layout::Overlay, scroll_default_size(horizontal));
            let mut semantics = SemanticNode::new(Role::ScrollView, "");
            semantics.scroll_axis = Some(if horizontal {
                crate::semantics::ScrollAxis::Horizontal
            } else {
                crate::semantics::ScrollAxis::Vertical
            });
            ui.semantics.borrow_mut().set(root, semantics);
            let contain = rare.as_deref().is_some_and(|r| r.overscroll_contain);
            scroll_offset = Some((offset, horizontal, contain));
            root
        }
        Kind::VirtualList(build) => {
            assert!(
                view.children.is_empty(),
                "virtual lists build children through their row factory"
            );
            // As a scroll view: 320 by 240 unless something sizes it (a
            // parent stretching it across, `grow`, its own `w`/`h`).
            let mut defaults = Styles::new().overflow_hidden();
            defaults.merge(&view.styles);
            view.styles = defaults;
            if keyboard_navigation && view.variants.focus.is_none() {
                view.variants.defaults[2] = Some(focus_ring(&view.styles));
            }
            region = Some(virtual_region(
                build,
                scrollbar,
                keyboard_navigation,
                rare.as_deref().is_some_and(|r| r.overscroll_contain),
                rare.as_deref_mut().and_then(|r| r.edges.take()),
            ));
            let root = ui.container(parent, Layout::Overlay, scroll_default_size(false));
            ui.semantics
                .borrow_mut()
                .set(root, SemanticNode::new(Role::ScrollView, ""));
            root
        }
        Kind::Region(build) => {
            region = Some(build);
            ui.container(parent, Layout::Column, Default::default())
        }
        _ => unreachable!(),
    };
    if view.contents {
        ui.scene.borrow_mut().set_contents(root, true);
    }
    if let Some(active) = rare.as_deref_mut().and_then(|r| r.active_descendant.take()) {
        watch_active_descendant(ui, root, environment.registry.clone(), active);
    }
    match rare.as_deref().and_then(|r| r.selectable) {
        Some(true) => crate::selection::mount_root(
            ui,
            root,
            rare.as_deref_mut()
                .and_then(|r| r.selection_style.take())
                .map(|style| *style)
                .unwrap_or_default(),
        ),
        Some(false) => ui.scene.borrow_mut().set_user_select(root, Some(false)),
        None => {}
    }
    let focusable = view
        .focusable
        .unwrap_or(interactive || is_editor || is_slider || keyboard_navigation);
    // Prepend in reverse so declaration order is preserved even when a primitive
    // constructor (such as an editor) already registered its default handlers.
    if let Some(preview) = rare.as_deref_mut().and_then(|r| r.drag_preview.take()) {
        crate::compose_drag::mount(ui, root, preview);
    }
    // A selectable region's own menu (Copy, Select All), or an editor's
    // (Cut, Copy, Paste, Select All), unless it has one.
    let context_menu = rare
        .as_deref_mut()
        .and_then(|r| r.context_menu.take())
        .or_else(|| {
            if let Some(editor) = &editor {
                return Some(crate::editor_menu::context_menu(
                    editor.clone(),
                    environment.services.get(),
                    ui.downgrade(),
                ));
            }
            (rare.as_deref().and_then(|r| r.selectable) == Some(true))
                .then(|| crate::selection::context_menu(ui, root, environment.services.get()))
        });
    if let Some(build) = context_menu {
        let style = rare
            .as_deref_mut()
            .and_then(|r| r.context_menu_style.take())
            .map(|style| *style);
        let below = rare
            .as_deref()
            .and_then(|r| r.context_menu_below)
            .unwrap_or(false);
        crate::compose_context_menu::mount(ui, root, build, style, below, environment.clone());
    }
    for callback in view.events.into_iter().rev() {
        let binding = ui.input.listen_first(root, callback);
        ui.retain(root, binding);
    }
    if let Some(r) = rare.as_deref()
        && r.mount_flags != 0
    {
        mount_drag_or_hollow(ui, root, r.mount_flags);
    }
    if let Some(rare) = rare.as_deref_mut()
        && (!rare.keymap.bindings.is_empty()
            || !rare.keymap.contexts.is_empty()
            || !rare.keymap.context.entries.is_empty())
    {
        let binding = ui.input.bind_keys(root, std::mem::take(&mut rare.keymap));
        ui.retain(root, binding);
    }
    if interactive {
        if !is_editor
            && !is_checkbox
            && !is_slider
            && !is_progress
            && !is_image
            && !is_scroll
            && !is_menu_item
        {
            ui.semantics
                .borrow_mut()
                .set(root, SemanticNode::new(Role::Button, ""));
        }
        let mut click = view.click.take();
        // A menu item's action runs once its menu has flashed and closed
        // (`compose_menu::flash`), so the flash shares it.
        let menu_click = item_menu
            .as_ref()
            .map(|_| Rc::new(RefCell::new(click.take())));
        let menu_trigger = rare.as_deref_mut().and_then(|r| r.menu_trigger.take());
        let keep_open = rare.as_deref().is_some_and(|r| r.menu_keep_open);
        let runner = item_menu
            .as_ref()
            .and_then(|_| environment.services.get::<TaskRunner>());
        let weak_ui = ui.downgrade();
        ui.on_event(root, true, move |cx| {
            // Over a menu's item (or pressing it): a submenu trigger opens
            // its submenu, and any other submenu of that menu closes.
            if cx.phase != EventPhase::Capture
                && matches!(
                    cx.event,
                    InputEvent::PointerEnter
                        | InputEvent::PointerDown {
                            button: PointerButton::Primary,
                            ..
                        }
                )
                && (cx.target == root || matches!(cx.event, InputEvent::PointerDown { .. }))
                && let Some(menu) = &item_menu
            {
                menu.close_children(menu_trigger.as_ref());
                // (A press opens it on activation, as a click always has.)
                if let Some(open) = &menu_trigger
                    && matches!(cx.event, InputEvent::PointerEnter)
                {
                    open.set(true);
                }
            }
            if cx.phase != EventPhase::Capture
                && matches!(cx.event, InputEvent::Activate)
                && !cx.default_prevented()
            {
                // A styled child may be the hit target. Its activation belongs
                // to the nearest clickable owner, not every outer button.
                cx.stop_propagation();
                if let Some(value) = &checkbox_value {
                    value.set(!value.get());
                }
                if let Some(open) = &menu_trigger {
                    // Hovering opened it already, and the press then focused
                    // this trigger: move focus back into the submenu, as
                    // opening it does.
                    if open.with_untracked(|o| *o)
                        && let Some(menu) = &item_menu
                        && let Some(ui) = weak_ui.upgrade()
                    {
                        menu.focus_submenu(&ui, open);
                    }
                    open.set(true);
                } else if let Some(menu) = &item_menu
                    && !keep_open
                {
                    // Where menus are windows of their own (AppKit's), the
                    // chosen item flashes, then the menu closes, then its
                    // action runs, as AppKit sends it.
                    if let (Some(runner), Some(ui)) = (&runner, weak_ui.upgrade())
                        && ui.popup_screens().with_untracked(|s| !s.is_empty())
                    {
                        let (click, modifiers) = (menu_click.clone(), cx.pointer_modifiers());
                        crate::compose_menu::flash(&ui, runner, menu.clone(), root, move || {
                            if let Some(click) = &click
                                && let Some(click) = click.borrow_mut().as_mut()
                            {
                                click(modifiers);
                            }
                        });
                        return;
                    }
                    menu.close_chain();
                }
                if let Some(click) = &mut click {
                    click(cx.pointer_modifiers());
                } else if let Some(click) = &menu_click
                    && let Some(click) = click.borrow_mut().as_mut()
                {
                    click(cx.pointer_modifiers());
                }
            }
        });
    }
    let scroll_layout_target = is_scroll.then(|| ui.signal(None));
    // A scroll view's layout follows its content; another's, its own.
    let layout_target = match is_scroll {
        true => scroll_layout_target.clone(),
        false => rare.as_deref_mut().and_then(|r| r.layout_target.take()),
    };
    let image_fit = is_image.then(|| ui.signal(crate::style::ImagePlacement::default()));
    environment.typography = mount_style_with_intrinsic(
        ui,
        root,
        view.styles,
        environment.typography,
        view.reactive_style,
        view.variants,
        view.disabled,
        focusable,
        image_intrinsic,
        layout_target,
        image_fit.clone(),
        Some(environment.theme.clone()),
    );
    if let Some(mut compute) = rich_compute {
        let paragraph = rich_paragraph.unwrap();
        let content_state = ui.signal(crate::compose_rich::Content {
            rich: std::sync::Arc::new(crate::rich_text::RichText::new("", Vec::new()).unwrap()),
            links: Vec::new(),
            icons: Vec::new(),
        });
        // The link under the pointer, restyled if it has a hover style.
        let hovered = ui.signal(None::<usize>);
        crate::compose_rich::mount_links(
            ui,
            root,
            paragraph,
            content_state.clone(),
            hovered.clone(),
            &environment,
        );
        crate::compose_rich::mount_icons(ui, root, paragraph, content_state.clone());
        let weak = ui.downgrade();
        let typography = environment.typography.clone();
        let theme = environment.theme.clone();
        ui.bind(root, move || {
            let base = typography.get();
            let spans = compute();
            let hovered = hovered.get();
            let mut content = String::new();
            let mut runs = Vec::new();
            let mut links = Vec::new();
            let mut icons = Vec::new();
            for mut span in spans {
                if span.text.is_empty() {
                    continue;
                }
                let link = span.action.is_some() || span.href.is_some();
                if link && span.hover.is_none() {
                    span.underline.get_or_insert_with(Default::default);
                }
                if let Some(hover) = span
                    .hover
                    .as_ref()
                    .filter(|_| link && hovered == Some(links.len()))
                {
                    if let Some(underline) = hover.underline {
                        span.underline = Some(underline);
                    }
                    if let Some(color) = hover.text_color {
                        span.styles = std::mem::take(&mut span.styles).text_color(color);
                    }
                }
                if span.styles.has_tokens() {
                    crate::theme::resolve(&mut span.styles, &theme);
                    span.background = span.background.or(span.styles.background);
                }
                // An icon keeps its room before the text: a padded
                // placeholder run of its own.
                let icon_at = span.icon.is_some().then(|| {
                    let at = content.len();
                    content.push_str(crate::compose_rich::ICON_PLACEHOLDER);
                    at..content.len()
                });
                let start = content.len();
                content.push_str(&span.text);
                if link {
                    links.push(crate::compose_rich::Link {
                        range: start..content.len(),
                        action: span.action,
                        href: span.href,
                        styled_hover: span.hover.is_some(),
                    });
                }
                let mut font = base.font_in(Some(&theme));
                let rare = span.styles.rare();
                let (fallbacks, features, spacing) = (
                    rare.font_fallbacks.clone(),
                    rare.font_features.clone(),
                    rare.letter_spacing,
                );
                if let Some(value) = fallbacks {
                    font.fallbacks = value;
                }
                if let Some(value) = features {
                    font.features = value;
                }
                if let Some(value) = span.styles.font_family.clone() {
                    font.family = value;
                }
                if let Some(value) = span.styles.font_weight {
                    font.weight = value;
                }
                if let Some(value) = span.styles.overflow().italic {
                    font.italic = value;
                }
                if let Some(value) = span.styles.line_height {
                    font.line_height = value;
                }
                if let Some(value) = spacing {
                    font.letter_spacing = value;
                }
                // An underlined paragraph underlines its runs (CSS's
                // `text-decoration` reaches inline children).
                let inherited = font.underline.map(|u| {
                    let line = crate::rich_text::Decoration::default();
                    u.offset().map_or(line, |offset| line.offset(offset))
                });
                let font_size = span
                    .styles
                    .text_size
                    .get()
                    .unwrap_or_else(|| base.size_in(Some(&theme)));
                let color = span
                    .styles
                    .text_color
                    .unwrap_or_else(|| base.color_in(Some(&theme)));
                // With an icon, the span's padding goes around both: its
                // room takes the padding before, a placeholder after the
                // text the padding after.
                let lead = if span.icon.is_some() {
                    span.padding[0]
                } else {
                    0.
                };
                if let (Some(range), Some(icon)) = (icon_at, span.icon.take()) {
                    runs.push(crate::rich_text::TextRun {
                        background: span.background,
                        underline: None,
                        strikethrough: None,
                        padding: [(lead + icon.size + icon.gap) / 2., span.padding[1]],
                        radius: span.radius,
                        range: range.clone(),
                        font: font.clone(),
                        font_size,
                        color,
                    });
                    icons.push(crate::compose_rich::InlineIcon {
                        range,
                        svg: match icon.tint {
                            Some(crate::theme::Themed::Value(tint)) => {
                                std::sync::Arc::new(icon.svg.tinted(tint))
                            }
                            Some(crate::theme::Themed::Token(token)) => std::sync::Arc::new(
                                icon.svg.tinted(
                                    theme
                                        .get(token)
                                        .color()
                                        .unwrap_or(crate::scene::Color(0, 0, 0, 0)),
                                ),
                            ),
                            None if icon.svg.follows_text_color() => {
                                std::sync::Arc::new(icon.svg.tinted(color))
                            }
                            None => icon.svg,
                        },
                        size: icon.size,
                        inset: lead,
                    });
                }
                let text_end = content.len();
                runs.push(crate::rich_text::TextRun {
                    background: span.background,
                    underline: {
                        let underline = span.underline.or(inherited);
                        match span.underline_color {
                            None => underline,
                            Some(crate::theme::Themed::Value(tint)) => {
                                underline.map(|d| d.color(tint))
                            }
                            Some(crate::theme::Themed::Token(token)) => underline.map(|d| {
                                d.color(
                                    theme
                                        .get(token)
                                        .color()
                                        .unwrap_or(crate::scene::Color(0, 0, 0, 0)),
                                )
                            }),
                        }
                    },
                    strikethrough: span.strikethrough,
                    padding: if lead > 0. {
                        [0., span.padding[1]]
                    } else {
                        span.padding
                    },
                    radius: span.radius,
                    range: start..text_end,
                    font: font.clone(),
                    font_size,
                    color,
                });
                if lead > 0. {
                    content.push_str(crate::compose_rich::ICON_PLACEHOLDER);
                    runs.push(crate::rich_text::TextRun {
                        background: span.background,
                        underline: None,
                        strikethrough: None,
                        padding: [lead / 2., span.padding[1]],
                        radius: span.radius,
                        range: text_end..content.len(),
                        font,
                        font_size,
                        color,
                    });
                }
            }
            let rich = crate::rich_text::RichText::new(content.as_str(), runs)
                .expect("normalized inline spans")
                .with_options(base.display);
            // Recomputed but unchanged (its inputs animate for a sibling):
            // nothing to lay out, paint, describe or relabel.
            if content_state.with_untracked(|current| {
                *current.rich == rich && current.links == links && current.icons == icons
            }) {
                return;
            }
            let Some(ui) = weak.upgrade() else {
                return;
            };
            if !ui.scene.borrow().contains(root) {
                return;
            }
            let rich = std::sync::Arc::new(rich);
            {
                let mut scene = ui.scene.borrow_mut();
                let mut style = scene.style(paragraph);
                style.text_wrap = base.wrap;
                scene.set_style(paragraph, style);
                scene.set_kind(paragraph, NodeKind::RichText { text: rich.clone() });
            }
            // Icons' placeholders aren't text to read out.
            let content = if icons.is_empty() {
                content
            } else {
                content.replace(crate::compose_rich::ICON_PLACEHOLDER, "")
            };
            content_state.set(crate::compose_rich::Content { rich, links, icons });
            ui.semantics.borrow_mut().update(paragraph, |node| {
                node.label = content.clone();
                node.value = Some(content);
            });
            update_button_labels(&ui, root);
        });
    }
    if let Some(mut source) = svg_compute {
        let child = ui.container(
            root,
            Layout::Overlay,
            crate::scene::Style {
                absolute: true,
                ..Default::default()
            },
        );
        // Its size arrives with the frame's layout (deferred: an icon
        // mounting forces no layout of its own, so a thread of hundreds
        // lays out once).
        let size = ui.observe_content_size_deferred(root);
        let scene = ui.scene.clone();
        let (typography, theme) = (environment.typography.clone(), environment.theme.clone());
        ui.bind(root, move || {
            let (width, height) = size.get();
            if width.is_nan() || height.is_nan() {
                return;
            }
            let mut data = source();
            // `currentColor`: the text colour here, as it changes.
            if data.follows_text_color() {
                data = std::sync::Arc::new(data.tinted(typography.get().color_in(Some(&theme))));
            }
            let mut scene = scene.borrow_mut();
            scene.set_kind(child, NodeKind::Svg(data));
            scene.set_style(
                child,
                crate::scene::Style {
                    width: Some(width),
                    height: Some(height),
                    absolute: true,
                    ..Default::default()
                },
            );
        });
    }
    let pictured = canvas_compute.is_some();
    let canvas_view = rare.as_deref_mut().and_then(|r| r.canvas_view.take());
    let grid_fill = rare.as_deref_mut().and_then(|r| r.grid_fill.take());
    assert!(
        (canvas_view.is_none() && grid_fill.is_none()) || canvas_compute.is_some(),
        "canvas_view and grid_fill are only supported on canvas views"
    );
    if let Some(mut draw) = canvas_compute {
        // As an icon's: with the frame's layout, forcing none.
        let size = ui.observe_content_size_deferred(root);
        let drawing = ui.scene.borrow_mut().append(
            root,
            NodeKind::Canvas(Default::default()),
            crate::scene::Style {
                absolute: true,
                ..Default::default()
            },
        );
        let scene = ui.scene.clone();
        let labels = Rc::new(RefCell::new(CanvasLabels::default()));
        let shown = labels.clone();
        ui.bind(root, move || {
            let (width, height) = size.get();
            if width.is_nan() || height.is_nan() {
                return;
            }
            let content = if width > 0. && height > 0. {
                crate::canvas::drawing_for(&scene, || draw((width, height)))
            } else {
                crate::canvas::Canvas::new()
            };
            let mut scene = scene.borrow_mut();
            if !content.texts().is_empty() || !shown.borrow().nodes.is_empty() {
                shown
                    .borrow_mut()
                    .show(&mut scene, root, content.texts(), (width, height));
            }
            scene.set_kind(drawing, NodeKind::Canvas(std::sync::Arc::new(content)));
            scene.set_style(
                drawing,
                crate::scene::Style {
                    width: Some(width),
                    height: Some(height),
                    absolute: true,
                    ..Default::default()
                },
            );
        });
        // Its pan and zoom, and the grid under it, apart from its drawing:
        // following them repaints it as it is.
        if let Some(mut view) = canvas_view {
            let scene = ui.scene.clone();
            ui.bind(root, move || {
                let view = view();
                let mut scene = scene.borrow_mut();
                scene.set_canvas_view(drawing, Some(view));
                labels.borrow_mut().follow(&mut scene, view);
            });
        }
        if let Some(mut grid) = grid_fill {
            let scene = ui.scene.clone();
            ui.bind(root, move || {
                let grid = grid();
                scene.borrow_mut().set_grid_fill(drawing, grid);
            });
        }
    }
    #[cfg(target_os = "macos")]
    if let Some(source) = native_source {
        mount_native_surface(
            ui,
            root,
            source,
            image_fit.clone().expect("surface fitting"),
        );
    }
    if let Some(source) = image_source {
        mount_image(ui, root, source.clone(), image_fit.expect("image fitting"));
        if let Some(animation) = rare.as_deref_mut().and_then(|r| r.animation.take()) {
            animation::mount(ui, root, source, animation, &environment);
        }
    }
    if let Some(value) = progress_state {
        mount_progress(
            ui,
            root,
            value,
            environment.typography.clone(),
            environment.services.get::<TaskRunner>(),
        );
    }
    if let Some((value, range, step)) = slider_state {
        mount_slider(ui, root, value, range, step, environment.typography.clone());
    }
    if let Some(editor) = editor {
        if let Some(mut read_only) = rare.as_deref_mut().and_then(|r| r.read_only.take()) {
            let editor = editor.clone();
            ui.bind(root, move || editor.set_read_only(read_only()));
        }
        if rare.as_deref().is_some_and(|r| r.overscroll_contain) {
            editor.set_overscroll_contain(true);
        }
        if let Some(select) = rare.as_deref().and_then(|r| r.select_on_focus) {
            editor.set_select_on_focus(select);
        }
        if let Some(secure) = rare.as_deref().and_then(|r| r.secure) {
            editor.set_secure(secure);
        }
        if let Some(mode) = rare.as_deref().and_then(|r| r.input_mode) {
            editor.set_input_mode(mode);
        }
        if let Some(check) = rare.as_deref_mut().and_then(|r| r.spellcheck.take())
            && rare.as_deref().and_then(|r| r.secure) != Some(true)
            && let Some(platform) = environment
                .services
                .get::<crate::platform::PlatformService>()
        {
            crate::editor_spell::mount(ui, editor.clone(), platform, check);
        }
        if let Some(numbered) = rare.as_deref().and_then(|r| r.line_numbers) {
            editor.set_line_numbers(numbered);
        }
        if let Some(code) = rare.as_deref_mut().and_then(|r| r.code.take()) {
            let CodeOptions {
                highlighter,
                decorations,
                underlines,
                current_line,
                indent,
                on_caret,
                on_top_line,
            } = *code;
            if highlighter.is_some() {
                editor.set_span_highlighter(highlighter);
                // Colours may be theme tokens: a new theme colours the lines
                // again (the highlighter is asked anew).
                let (editor, theme) = (editor.clone(), environment.theme.clone());
                let first = Cell::new(true);
                ui.bind(root, move || {
                    theme.color(crate::theme::tok::TEXT_PRIMARY);
                    theme.color(crate::theme::tok::ACCENT);
                    if !first.replace(false) {
                        editor.rehighlight();
                    }
                });
            }
            if let Some(told) = on_caret {
                editor.on_caret(told);
            }
            if let Some(told) = on_top_line {
                editor.on_top_line(told);
            }
            editor.set_indent(indent.as_deref());
            if let Some(mut ranges) = decorations {
                let editor = editor.clone();
                ui.bind(root, move || editor.set_decorations(ranges()));
            }
            if let Some(mut ranges) = underlines {
                let editor = editor.clone();
                ui.bind(root, move || editor.set_underlines(ranges()));
            }
            if let Some(mut color) = current_line {
                let editor = editor.clone();
                ui.bind(root, move || editor.set_current_line(color()));
            }
        }
        let caret_width = rare.as_deref().and_then(|r| r.caret_width);
        match rare.as_deref_mut().and_then(|r| r.caret_color.take()) {
            Some(mut color) => {
                let editor = editor.clone();
                ui.bind(root, move || editor.set_caret_style(color(), caret_width));
            }
            None if caret_width.is_some() => editor.set_caret_style(None, caret_width),
            None => {}
        }
        let (mut text, mut color) = (
            rare.as_deref_mut().and_then(|r| r.placeholder.take()),
            rare.as_deref_mut().and_then(|r| r.placeholder_color.take()),
        );
        if text.is_some() {
            let editor = editor.clone();
            ui.bind(root, move || {
                let text = text.as_mut().map(|text| text());
                let color = color.as_mut().map(|color| color());
                editor.set_placeholder(text.as_deref(), color);
            });
        }
        let typography = environment.typography.clone();
        let theme = environment.theme.clone();
        ui.bind_input(root, move || {
            let style = typography.get();
            let theme = Some(&*theme);
            editor.set_text_style(
                style.color_in(theme),
                style.size_in(theme),
                style.font_in(theme),
                style.wrap,
            );
        });
    }
    if let Some(id) = view.id {
        environment
            .registry
            .borrow_mut()
            .entry(id.clone())
            .or_default()
            .push(root);
        ui.retain(
            root,
            IdRegistration {
                registry: environment.registry.clone(),
                id,
                node: root,
            },
        );
    }
    let scroll_nodes = scroll_offset.as_ref().map(|(_, horizontal, _)| {
        let viewport = ui.container(root, Layout::Overlay, Default::default());
        let content = ui.container(
            viewport,
            if *horizontal {
                Layout::Row
            } else {
                Layout::Column
            },
            Default::default(),
        );
        ui.scene.borrow_mut().set_scroll_copy(content, true);
        scroll_layout_target.as_ref().unwrap().set(Some(content));
        (viewport, content)
    });
    let child_parent = scroll_nodes.map_or(root, |(_, content)| content);
    Opened {
        children: view.children,
        conditional: rare
            .as_deref_mut()
            .map(|r| std::mem::take(&mut r.conditional))
            .unwrap_or_default(),
        row_text: rare.as_deref_mut().and_then(|r| r.row_text.take()),
        pointer_events: rare.as_deref_mut().and_then(|r| r.pointer_events.take()),
        accessible_label: rare.as_deref_mut().and_then(|r| r.accessible_label.take()),
        accessibility: rare.and_then(|r| r.accessibility.take()),
        mounted: view.mounted,
        focusable_set: view.focusable.is_some(),
        environment,
        root,
        child_parent,
        submenu_indicator,
        scroll_offset,
        scroll_nodes,
        scrollbar,
        follow_end,
        compute,
        region,
        focusable,
        pictured,
    }
}
/// `View::active_descendant`: `root`'s semantics point at the node named
/// what `id` says, found again as `id` changes, and after the next layout
/// when it is not mounted yet (an item in a popover just opening).
#[inline(never)]
fn watch_active_descendant(
    ui: &mut Ui,
    root: NodeId,
    registry: Registry,
    id: Box<dyn Fn() -> Option<String>>,
) {
    fn point(ui: &Ui, root: NodeId, registry: &Registry, id: Option<&str>) -> bool {
        let target = id.and_then(|id| {
            let scene = ui.scene.borrow();
            registry
                .borrow()
                .get(id)?
                .iter()
                .copied()
                .find(|node| scene.contains(*node))
        });
        let mut semantics = ui.semantics.borrow_mut();
        if semantics.get(root).is_none() {
            return false;
        }
        if semantics
            .get(root)
            .is_some_and(|node| node.active_descendant != target)
        {
            semantics.update(root, |node| node.active_descendant = target);
        }
        target.is_some() || id.is_none()
    }
    let weak = ui.downgrade();
    ui.bind(root, move || {
        let id = id();
        let Some(ui) = weak.upgrade() else { return };
        if !point(&ui, root, &registry, id.as_deref()) {
            let (weak, registry) = (weak.clone(), registry.clone());
            ui.after_layout(Box::new(move || {
                if let Some(ui) = weak.upgrade() {
                    point(&ui, root, &registry, id.as_deref());
                }
            }));
        }
    });
}
/// The rest of an element's mounting, once its children are in.
#[cfg_attr(debug_assertions, inline(never))]
#[cfg_attr(not(debug_assertions), inline(always))]
fn close_element(ui: &mut Ui, opened: Opened) -> NodeId {
    let Opened {
        children: _,
        conditional,
        mut row_text,
        pointer_events,
        accessible_label,
        accessibility,
        mounted,
        focusable_set,
        environment,
        root,
        child_parent,
        submenu_indicator,
        scroll_offset,
        scroll_nodes,
        scrollbar,
        follow_end,
        compute,
        region,
        focusable,
        pictured,
    } = opened;
    for conditional in conditional {
        conditional(ui, child_parent, environment.clone());
    }
    if submenu_indicator {
        let indicator = mount(ui, root, text("›"), environment.clone());
        ui.semantics.borrow_mut().remove(indicator);
    }
    if let (Some((offset, horizontal, contain)), Some((viewport, content))) =
        (scroll_offset, scroll_nodes)
    {
        mount_scroll(
            ui, root, viewport, content, offset, horizontal, scrollbar, follow_end, contain,
        );
    }
    if let Some(mut compute) = compute {
        let weak = ui.downgrade();
        ui.bind(root, move || {
            let value = compute();
            let Some(ui) = weak.upgrade() else {
                return;
            };
            if !ui.scene.borrow().contains(root) {
                return;
            }
            ui.scene.borrow_mut().set_text(root, value.as_str());
            // Only assistive technology reads it: while none does, later.
            if !ui.defer_label(root) {
                ui.semantics.borrow_mut().set_label(root, &value);
                update_button_labels(&ui, root);
            }
        });
    }
    assert!(
        row_text.is_none() || region.is_some(),
        "row_text is only supported on virtual lists"
    );
    if let Some(build) = region {
        build(ui, root, environment);
    }
    if let Some(text) = row_text.take() {
        crate::selection::mount_row_text(ui, root, text);
    }
    let mut options = ui.input.options(root).unwrap_or_default();
    options.focusable = focusable;
    if focusable_set || ui.input.options(root).is_some() {
        ui.input.set_options(root, options);
    }
    if let Some(mut enabled) = pointer_events {
        let weak = ui.downgrade();
        ui.bind(root, move || {
            let pass_through = !enabled();
            let Some(ui) = weak.upgrade() else {
                return;
            };
            if !ui.scene.borrow().contains(root) {
                return;
            }
            let mut options = ui.input.options(root).unwrap_or_default();
            if options.pass_through != pass_through || ui.input.options(root).is_none() {
                options.pass_through = pass_through;
                ui.input.set_options(root, options);
            }
        });
    }
    // What a box named or described is, unless it says: a group, or a
    // canvas's picture.
    let role = if pictured { Role::Image } else { Role::Group };
    if let Some(label) = accessible_label {
        let mut semantics = ui.semantics.borrow_mut();
        if semantics.get(root).is_some() {
            semantics.update(root, |node| {
                node.label = label;
                node.named = true;
            });
        } else {
            let mut node = SemanticNode::new(role, label);
            node.named = true;
            semantics.set(root, node);
        }
    }
    if let Some(accessibility) = accessibility {
        // A button given another role (a radio, a menu item) keeps the name
        // its content gives it as a button.
        if accessibility.role.is_some() {
            name_from_content(ui, &ui.scene.borrow(), root);
        }
        apply_accessibility(ui, root, *accessibility, role);
    }
    ui.close_element(root);
    update_button_labels(ui, root);
    for mounted in mounted {
        mounted(NodeHandle {
            ui: ui.downgrade(),
            node: root,
        });
    }
    root
}
/// `Rare::mount_flags`: the region drags the window.
const WINDOW_DRAG: u8 = 1;
/// `Rare::mount_flags`: the pointer passes through it while nothing it holds
/// is displayed.
const HOLLOW: u8 = 2;
/// `View::window_drag` and `View::hollow`, rare: apart from the element's
/// common path.
#[cold]
#[inline(never)]
fn mount_drag_or_hollow(ui: &mut Ui, root: NodeId, flags: u8) {
    if flags & WINDOW_DRAG != 0 {
        mount_window_drag(ui, root);
    }
    // Registered, so the pointer's walk looks at it, and marked.
    if flags & HOLLOW != 0 {
        ui.input
            .set_options(root, ui.input.options(root).unwrap_or_default());
        ui.input.set_hollow(root);
    }
}
/// `View::window_drag`: a primary press on `region`, or on a descendant
/// that takes no input itself, asks the host to move the window (or, the
/// second of a double click, to zoom it); a secondary click, for the
/// window's menu.
fn mount_window_drag(ui: &mut Ui, region: NodeId) {
    let weak = ui.downgrade();
    ui.on_event(region, false, move |cx| {
        let menu = matches!(
            cx.event,
            InputEvent::PointerUp {
                button: PointerButton::Secondary,
                ..
            }
        );
        if cx.phase == EventPhase::Capture
            || cx.default_prevented()
            || !(menu
                || matches!(
                    cx.event,
                    InputEvent::PointerDown {
                        button: PointerButton::Primary,
                        ..
                    }
                ))
        {
            return;
        }
        let Some(ui) = weak.upgrade() else {
            return;
        };
        // A descendant following the pointer (a divider) keeps its press.
        if cx.capture_target().is_some_and(|id| id != Some(region)) {
            return;
        }
        // So does anything inside that takes input (a button, an input).
        {
            let scene = ui.scene.borrow();
            let mut node = Some(cx.target);
            while let Some(id) = node.filter(|id| *id != region) {
                if ui
                    .input
                    .options(id)
                    .is_some_and(|options| options.focusable)
                {
                    return;
                }
                node = scene.parent(id);
            }
        }
        // Moving the window neither focuses nor presses what is under it.
        cx.prevent_default();
        cx.stop_propagation();
        ui.request_window_gesture(if menu {
            crate::widgets::WindowGesture::Menu
        } else if cx.click_count() == 2 {
            crate::widgets::WindowGesture::DoubleClick
        } else {
            crate::widgets::WindowGesture::Drag
        });
    });
}
/// A list's stretch past its edges under a trackpad (`elastic`), as a
/// scroll view's: what shows of its content is where the list placed it
/// ([`ListStretch::place`]) moved by the stretch, in a binding of its own,
/// so a frame of stretching or springing back places no rows. At an edge
/// the rows showing are the same.
pub(crate) struct ListStretch {
    content: NodeId,
    placed: Rc<Cell<f32>>,
    shown: Signal<f32>,
}
impl ListStretch {
    /// `root`'s list, its rows in `content`, scrolled to `offset` within
    /// its `size`, `extent` tall.
    pub(crate) fn new(
        ui: &mut Ui,
        root: NodeId,
        content: NodeId,
        offset: Signal<f32>,
        size: Signal<(f32, f32)>,
        extent: Signal<f32>,
    ) -> Self {
        let extents = ui.signal((0_f32, 0_f32));
        {
            let (size, extents) = (size.clone(), extents.clone());
            ui.bind(root, move || {
                let extent = (size.get().0, extent.get());
                if extents.with_untracked(|e| *e != extent) {
                    extents.set(extent);
                }
            });
        }
        let shown = crate::elastic::register(ui, root, false, size, extents, offset);
        let placed = Rc::new(Cell::new(0_f32));
        {
            let (placed, shown, scene) = (placed.clone(), shown.clone(), ui.scene.clone());
            let pixel_scale = ui.pixel_scale();
            let mut first = true;
            ui.bind(root, move || {
                let stretch = shown.get();
                // Placed again from where it was asked to be at a new scale.
                let _ = pixel_scale.get();
                // The list places its content as it mounts (`place`).
                if std::mem::take(&mut first) {
                    return;
                }
                let mut scene = scene.borrow_mut();
                if scene.contains(content) {
                    scene.set_transform(
                        content,
                        crate::scene::Transform {
                            x: 0.,
                            y: -(placed.get() + stretch),
                        },
                    );
                }
            });
        }
        Self {
            content,
            placed,
            shown,
        }
    }
    /// The list scrolled its content to `position`.
    pub(crate) fn place(&self, scene: &mut crate::scene::Scene, position: f32) {
        self.placed.set(position);
        let stretch = self.shown.with_untracked(|s| *s);
        scene.set_transform(
            self.content,
            crate::scene::Transform {
                x: 0.,
                y: -(position + stretch),
            },
        );
    }
}
#[allow(clippy::too_many_arguments)]
fn mount_scroll(
    ui: &mut Ui,
    root: NodeId,
    viewport: NodeId,
    content: NodeId,
    offset: Signal<f32>,
    horizontal: bool,
    scrollbar: bool,
    follow_end: bool,
    // `View::overscroll_contain`: a wheel at its edge stays here.
    contain: bool,
) {
    use crate::scene::{Style, Transform};
    let main_size = move |size: (f32, f32)| if horizontal { size.0 } else { size.1 };
    // The offset counts from the start: right to left, a horizontal
    // view's start is its right, and moving on reveals what is left of it.
    let translation = move |offset: f32, rtl: bool| {
        if horizontal {
            Transform {
                x: if rtl { offset } else { -offset },
                y: 0.,
            }
        } else {
            Transform { x: 0., y: -offset }
        }
    };
    let rtl_of = move |scene: &crate::scene::Scene| {
        horizontal && scene.direction(root) == crate::scene::Direction::Rtl
    };
    let size = ui.observe_content_size(root);
    let extent = ui.observe_content_size(content);
    // What a trackpad stretches it by past an edge (`elastic`): zero unless
    // the host scrolls elastically.
    let stretch = crate::elastic::register(
        ui,
        root,
        horizontal,
        size.clone(),
        extent.clone(),
        offset.clone(),
    );
    if scrollbar {
        let main_extent = ui.signal(main_size(extent.get()));
        let read_extent = extent.clone();
        let write_extent = main_extent.clone();
        ui.bind(root, move || {
            write_extent.set(main_size(read_extent.get()));
        });
        crate::compose_scrollbar::mount(
            ui,
            root,
            offset.clone(),
            size.clone(),
            main_extent,
            horizontal,
        );
    }
    let wheel_size = size.clone();
    let wheel_extent = extent.clone();
    let wheel_offset = offset.clone();
    let weak = ui.downgrade();
    ui.on_event(root, false, move |cx| {
        if cx.phase == EventPhase::Capture {
            return;
        }
        if matches!(cx.event, InputEvent::Focus) && cx.target != root {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            // Direct focus calls need the same settled allocation as native input.
            // World bounds include transforms immediately, so an outer scroller
            // observes the translation applied by an inner bubbling handler.
            ui.prepare_frame();
            // A target handler may redirect focus before this event bubbles.
            if ui.input.focused() != Some(cx.target) {
                return;
            }
            let movement = {
                let scene = ui.scene.borrow();
                if !scene.contains(cx.target) || !scene.contains(viewport) {
                    return;
                }
                let target = scene.bounds(cx.target);
                let visible = scene.bounds(viewport);
                let (target_start, target_size, visible_start, visible_size) = if horizontal {
                    (target.x, target.width, visible.x, visible.width)
                } else {
                    (target.y, target.height, visible.y, visible.height)
                };
                let start_delta = target_start as f64 - visible_start as f64;
                let end_delta = target_start as f64 + target_size as f64
                    - visible_start as f64
                    - visible_size as f64;
                if start_delta < 0. && end_delta > 0. {
                    0. // Oversized targets already spanning the viewport stay put.
                } else if start_delta < 0. {
                    if target_size > visible_size {
                        end_delta
                    } else {
                        start_delta
                    }
                } else if end_delta > 0. {
                    if target_size > visible_size {
                        start_delta
                    } else {
                        end_delta
                    }
                } else {
                    0.
                }
            };
            let limit =
                (main_size(wheel_extent.get()) as f64 - main_size(wheel_size.get()) as f64).max(0.);
            let rtl = rtl_of(&ui.scene.borrow());
            let movement = if rtl { -movement } else { movement };
            let position = (wheel_offset.get() as f64 + movement).clamp(0., limit) as f32;
            wheel_offset.set(position);
            // A surrounding reactive batch can defer the geometry binding.
            // Publish this translation now for the next ancestor's focus handler.
            let mut scene = ui.scene.borrow_mut();
            if scene.contains(content) && ui.input.focused() == Some(cx.target) {
                let rtl = rtl_of(&scene);
                scene.set_transform(content, translation(wheel_offset.get(), rtl));
            }
        }
        if let InputEvent::Scroll {
            delta_x, delta_y, ..
        } = cx.event
        {
            // A Control-wheel zooms the window instead.
            if cx.default_prevented() || crate::zoom::wheel_zooms() {
                return;
            }
            let delta = if horizontal { delta_x } else { delta_y };
            if !delta.is_finite() {
                return;
            }
            // A wheel's horizontal delta is physical: to the right is back
            // toward the start, right to left.
            let delta = match weak.upgrade() {
                Some(ui) if rtl_of(&ui.scene.borrow()) => -delta,
                _ => delta,
            };
            let before = wheel_offset.get();
            let limit =
                (main_size(wheel_extent.get()) as f64 - main_size(wheel_size.get()) as f64).max(0.);
            let after = (before as f64 + delta as f64).clamp(0., limit) as f32;
            if before != after {
                wheel_offset.set(after);
                cx.scrolled(horizontal);
            } else if contain && delta != 0. {
                // At its edge, contained: taken all the same, so it latches
                // here and the views around it never see it.
                cx.scrolled(horizontal);
            }
        }
    });
    let scene = ui.scene.clone();
    // For `follow_end`: the position last applied, and whether it was the
    // end. Only growth moves a pinned view; a new offset (a wheel, a script)
    // is taken as it is and decides whether the view stays pinned.
    let (applied, pinned) = (std::cell::Cell::new(0_f32), std::cell::Cell::new(true));
    let pixel_scale = ui.pixel_scale();
    ui.bind(root, move || {
        // Placed again from where it was asked to be at a new scale.
        let _ = pixel_scale.get();
        let (width, height) = size.get();
        let limit = (main_size(extent.get()) - main_size((width, height))).max(0.);
        let requested = offset.get();
        let position = if requested.is_nan() {
            0.
        } else if follow_end && pinned.get() && requested == applied.get() {
            limit
        } else {
            requested.clamp(0., limit)
        };
        if follow_end {
            applied.set(position);
            pinned.set(position >= limit - 0.5);
        }
        if requested != position {
            offset.set(position);
        }
        let mut scene = scene.borrow_mut();
        // Scrolling across with no height of its own, it is as tall as what
        // it holds (its measured height would be its content's, circularly:
        // 0), or as a parent stretching it makes it: the viewport and the
        // content take all of a definite height, none of an indefinite one.
        let fitted = horizontal && {
            let style = scene.style(root);
            style.height.is_none() && style.height_percent.is_none()
        };
        scene.set_style(
            viewport,
            Style {
                width: Some(width),
                height: (!fitted).then_some(height),
                height_percent: fitted.then_some(1.),
                clip: true,
                ..Default::default()
            },
        );
        let mut content_style = scene.style(content);
        content_style.width = (!horizontal).then_some(width);
        content_style.height = (horizontal && !fitted).then_some(height);
        content_style.height_percent = fitted.then_some(1.);
        content_style.flex_shrink = 0.;
        scene.set_style(content, content_style);
        let rtl = rtl_of(&scene);
        scene.set_transform(content, translation(position + stretch.get(), rtl));
    });
}
#[cfg(target_os = "macos")]
fn mount_native_surface(
    ui: &mut Ui,
    root: NodeId,
    source: Signal<Option<std::rc::Rc<crate::native_surface::NativeSurface>>>,
    fit: Signal<crate::style::ImagePlacement>,
) {
    use crate::scene::{Style, Transform};
    let viewport = ui.container(
        root,
        Layout::Overlay,
        Style {
            absolute: true,
            clip: true,
            ..Default::default()
        },
    );
    let bitmap = ui.container(
        viewport,
        Layout::Overlay,
        Style {
            absolute: true,
            ..Default::default()
        },
    );
    let size = ui.observe_content_size(root);
    let scene = ui.scene.clone();
    ui.bind(root, move || {
        let frame = source.get().expect("mounted surface");
        let (width, height) = size.get();
        let (w, h) = fitted_image_size(
            width,
            height,
            frame.width() as f32,
            frame.height() as f32,
            fit.get().fit,
        );
        let mut scene = scene.borrow_mut();
        scene.set_kind(bitmap, NodeKind::NativeSurface(frame));
        scene.set_style(
            viewport,
            Style {
                width: Some(width),
                height: Some(height),
                absolute: true,
                clip: true,
                ..Default::default()
            },
        );
        scene.set_style(
            bitmap,
            Style {
                width: Some(w),
                height: Some(h),
                absolute: true,
                ..Default::default()
            },
        );
        scene.set_transform(
            bitmap,
            Transform {
                x: (width - w) * 0.5,
                y: (height - h) * 0.5,
            },
        );
    });
}
/// A GPU view's box: a frame the size of its content box, made anew when
/// that size changes, the view asks (`GpuView::invalidate`) or, while it
/// animates and shows, every refresh.
fn mount_gpu_view(
    ui: &mut Ui,
    root: NodeId,
    view: crate::gpu_view::GpuView,
    runner: Option<Rc<TaskRunner>>,
    frames: Option<Rc<crate::frame::FrameClock>>,
) {
    use crate::scene::Style;
    let viewport = ui.container(
        root,
        Layout::Overlay,
        Style {
            absolute: true,
            clip: true,
            ..Default::default()
        },
    );
    // A frame once laid out; nothing to draw until then.
    let frame = ui.container(
        viewport,
        Layout::Overlay,
        Style {
            absolute: true,
            ..Default::default()
        },
    );
    let requests = ui.signal(0_u64);
    let animating = ui.signal(false);
    let mounted = view.mount(requests.clone(), animating.clone());
    let shown_by = mounted.key();
    ui.retain(root, mounted);
    // Shown, its painter lives: the app may keep no handle of its own.
    ui.retain(root, view.clone());
    let size = ui.observe_content_size(root);
    let scene = ui.scene.clone();
    let id = view.id();
    let mut ring = crate::gpu_view::FrameRing::default();
    let mut shown = None;
    let asked = requests.clone();
    let view_opaque = view.clone();
    ui.bind(root, move || {
        asked.get();
        let (width, height) = size.get();
        let mut scene = scene.borrow_mut();
        if !(width > 0. && height > 0.) {
            if shown.take().is_some() {
                scene.set_kind(frame, NodeKind::Container(Layout::Overlay));
            }
            return;
        }
        let image = ring.next(crate::gpu_view::GpuFrame {
            view: id,
            shown_by,
            width,
            height,
            opaque: view_opaque.opaque(),
        });
        if shown.replace((width, height)) == Some((width, height)) {
            // On a plane of its own, the window has nothing to repaint.
            if crate::gpu_view::planed(id, shown_by) {
                scene.show_plane_frame(frame, image);
            } else {
                scene.set_image_frame(frame, image);
            }
            return;
        }
        if matches!(scene.kind(frame), NodeKind::Image(_)) {
            scene.set_image_frame(frame, image);
        } else {
            scene.set_kind(frame, NodeKind::Image(image));
        }
        let boxed = Style {
            width: Some(width),
            height: Some(height),
            absolute: true,
            ..Default::default()
        };
        scene.set_style(
            viewport,
            Style {
                clip: true,
                ..boxed.clone()
            },
        );
        scene.set_style(frame, boxed);
    });
    // Animating, a frame each refresh while it shows; otherwise no timer.
    let (Some(runner), Some(frames)) = (runner, frames) else {
        return;
    };
    let frames = (*frames).clone();
    let visible = ui.observe_visibility(root);
    let presented = ui.observe_presentation();
    let task: Rc<RefCell<Option<TaskToken>>> = Default::default();
    ui.retain(root, task.clone());
    ui.bind(root, move || {
        let active = animating.get() && presented.get() && visible.get();
        drop(task.borrow_mut().take());
        if !active {
            return;
        }
        let frames = frames.clone();
        let requests = requests.clone();
        *task.borrow_mut() = Some(runner.spawn(Box::pin(async move {
            loop {
                frames.next().await;
                requests.mutate(|count| {
                    *count = count.wrapping_add(1);
                    true
                });
            }
        })));
    });
}
fn mount_image(
    ui: &mut Ui,
    root: NodeId,
    source: Signal<Option<std::sync::Arc<crate::image::ImageData>>>,
    fit: Signal<crate::style::ImagePlacement>,
) {
    use crate::scene::{Style, Transform};
    let viewport = ui.container(
        root,
        Layout::Overlay,
        Style {
            absolute: true,
            clip: true,
            ..Default::default()
        },
    );
    // The source binding may be deferred by the surrounding mount batch.
    // Reserve a stable node now; that binding supplies its pixels before paint.
    let initial = source
        .get()
        .map(NodeKind::Image)
        .unwrap_or(NodeKind::Container(Layout::Overlay));
    let bitmap = ui.scene.borrow_mut().append(
        viewport,
        initial,
        Style {
            absolute: true,
            ..Default::default()
        },
    );
    let size = ui.observe_content_size(root);
    let scene = ui.scene.clone();
    ui.bind(root, move || {
        let data = source.get().expect("mounted image source");
        let (width, height) = size.get();
        let placement = fit.get();
        let fitting = placement.fit;
        let crop = data.transform() == crate::affine::Affine::IDENTITY
            || matches!(
                fitting,
                crate::style::ObjectFit::Cover | crate::style::ObjectFit::None
            );
        let (image_width, image_height) = fitted_image_size(
            width,
            height,
            data.width() as f32,
            data.height() as f32,
            fitting,
        );
        let mut scene = scene.borrow_mut();
        scene.set_kind(bitmap, NodeKind::Image(data));
        // A crop is a picture past the box: its corners cut that too.
        scene.set_corners_clip(root, crop);
        scene.set_style(
            viewport,
            Style {
                width: Some(width),
                height: Some(height),
                absolute: true,
                clip: crop,
                ..Default::default()
            },
        );
        scene.set_style(
            bitmap,
            Style {
                width: Some(image_width),
                height: Some(image_height),
                absolute: true,
                ..Default::default()
            },
        );
        scene.set_transform(
            bitmap,
            Transform {
                x: (width - image_width) * placement.position.0,
                y: (height - image_height) * placement.position.1,
            },
        );
    });
}
fn fitted_image_size(
    width: f32,
    height: f32,
    source_width: f32,
    source_height: f32,
    fit: crate::style::ObjectFit,
) -> (f32, f32) {
    use crate::style::ObjectFit;
    if width <= 0. || height <= 0. {
        return (0., 0.);
    }
    if fit == ObjectFit::Fill {
        return (width, height);
    }
    if fit == ObjectFit::None {
        return (source_width, source_height);
    }
    let horizontal = width as f64 / source_width as f64;
    let vertical = height as f64 / source_height as f64;
    let scale = match fit {
        ObjectFit::Cover => horizontal.max(vertical),
        ObjectFit::ScaleDown => horizontal.min(vertical).min(1.),
        _ => horizontal.min(vertical),
    };
    (
        (source_width as f64 * scale).min(f32::MAX as f64) as f32,
        (source_height as f64 * scale).min(f32::MAX as f64) as f32,
    )
}
/// How long an indeterminate bar takes to cross, and how much of the track
/// it covers.
const SWEEP: std::time::Duration = std::time::Duration::from_millis(1400);
const SWEEP_WIDTH: f32 = 0.3;
/// A progress bar's fill: `value` of its width, or (NaN, "indeterminate")
/// a stretch of it sweeping across while the work's size is unknown, as
/// AppKit's and the web's do; still headless (no runner).
fn mount_progress(
    ui: &mut Ui,
    root: NodeId,
    value: Signal<f32>,
    typography: Signal<Typography>,
    runner: Option<Rc<TaskRunner>>,
) {
    use crate::scene::{Style, Transform};
    let size = ui.observe_content_size(root);
    let viewport = ui.container(
        root,
        Layout::Overlay,
        Style {
            clip: true,
            ..Default::default()
        },
    );
    let fill = ui.scene.borrow_mut().append(
        viewport,
        NodeKind::Rect(typography.with_untracked(|t| t.color_in(None))),
        Style::default(),
    );
    let theme = ui.theme_scope();
    let scene = ui.scene.clone();
    let semantics = ui.semantics.clone();
    let sweeping: Rc<RefCell<Option<TaskToken>>> = Rc::default();
    // Where the sweep is, 0 to 1 across: a task moves it a frame at a time.
    let phase = ui.signal(0_f32);
    ui.bind(root, move || {
        let raw = value.get();
        let indeterminate = raw.is_nan();
        let normalized = if indeterminate { 0. } else { raw.clamp(0., 1.) };
        if !indeterminate && raw != normalized {
            value.set(normalized);
        }
        let (width, height) = size.get();
        let mut scene = scene.borrow_mut();
        let geometry = Style {
            width: Some(width),
            height: Some(height),
            ..Default::default()
        };
        scene.set_style(
            viewport,
            Style {
                clip: true,
                ..geometry.clone()
            },
        );
        scene.set_kind(
            fill,
            NodeKind::Rect(typography.get().color_in(Some(&theme))),
        );
        let rtl = scene.direction(root) == crate::scene::Direction::Rtl;
        if indeterminate {
            let stretch = width * SWEEP_WIDTH;
            scene.set_style(
                fill,
                Style {
                    width: Some(stretch),
                    ..geometry
                },
            );
            // From the start across, entering and leaving off the track.
            let t = phase.get();
            let eased = t * t * (3. - 2. * t);
            let x = -stretch + eased * (width + stretch);
            scene.set_transform(
                fill,
                Transform {
                    x: if rtl { width - stretch - x } else { x },
                    y: 0.,
                },
            );
            if sweeping.borrow().is_none()
                && let Some(runner) = &runner
            {
                let (phase, start) = (phase.clone(), std::time::Instant::now());
                *sweeping.borrow_mut() = Some(runner.spawn(Box::pin(async move {
                    loop {
                        crate::timer::sleep(std::time::Duration::from_millis(16)).await;
                        let t = (start.elapsed().as_secs_f32() / SWEEP.as_secs_f32()).fract();
                        phase.set(t);
                    }
                })));
            }
        } else {
            sweeping.borrow_mut().take();
            scene.set_style(fill, geometry);
            // A translated full-size fill stays clipped to the content
            // viewport, so value-only updates change no layout. Right to
            // left it fills from the right, as a browser's does.
            let hidden = width * (1. - normalized);
            scene.set_transform(
                fill,
                Transform {
                    x: if rtl { hidden } else { -hidden },
                    y: 0.,
                },
            );
        }
        semantics.borrow_mut().update(root, |node| {
            node.numeric_value = (!indeterminate).then_some(normalized as f64)
        });
    });
}
fn mount_slider(
    ui: &mut Ui,
    root: NodeId,
    value: Signal<f32>,
    range: std::ops::RangeInclusive<f32>,
    step_size: Option<f64>,
    typography: Signal<Typography>,
) {
    use crate::{
        input::{Key, PointerButton},
        scene::{Style, Transform},
    };
    let (min, max) = (*range.start() as f64, *range.end() as f64);
    let span = max - min;
    let size = ui.observe_content_size(root);
    let theme = ui.theme_scope();
    let rail = ui.scene.borrow_mut().append(
        root,
        NodeKind::Rect(theme.color_untracked(tok::HOVER)),
        Style::default(),
    );
    let fill_viewport = ui.container(
        root,
        Layout::Overlay,
        Style {
            clip: true,
            ..Default::default()
        },
    );
    let fill = ui.scene.borrow_mut().append(
        fill_viewport,
        NodeKind::Rect(theme.color_untracked(tok::ACCENT)),
        Style::default(),
    );
    let thumb = ui.scene.borrow_mut().append(
        root,
        NodeKind::Rect(typography.with_untracked(|t| t.color_in(None))),
        Style::default(),
    );
    let thumb_theme = theme.clone();
    // The rail and fill follow the theme: the effect reads their tokens.
    {
        let scene = ui.scene.clone();
        ui.bind(root, move || {
            let mut scene = scene.borrow_mut();
            scene.set_kind(rail, NodeKind::Rect(theme.color(tok::HOVER)));
            scene.set_kind(fill, NodeKind::Rect(theme.color(tok::ACCENT)));
        });
    }
    let scene = ui.scene.clone();
    let semantics = ui.semantics.clone();
    let observed_value = value.clone();
    ui.bind(root, move || {
        let raw = observed_value.get();
        let val = if raw.is_nan() {
            min as f32
        } else {
            raw.clamp(min as f32, max as f32)
        };
        if raw != val {
            observed_value.set(val);
        }
        let (width, height) = size.get();
        let thumb_width = width.min(10.);
        let thumb_height = height.min(20.);
        let travel = (width - thumb_width).max(0.);
        let x = ((val as f64 - min) / span * travel as f64) as f32;
        let rail_height = height.min(4.);
        let fixed = |w, h| Style {
            width: Some(w),
            height: Some(h),
            ..Default::default()
        };
        let mut scene = scene.borrow_mut();
        // Right to left, it fills from the right, as a browser's range input
        // does: where it shows at `p` from the left. The scene lays a child
        // `w` wide in at the right, so its translation is from there.
        let rtl = scene.direction(root) == crate::scene::Direction::Rtl;
        let at = |w: f32, p: f32| if rtl { p - (width - w) } else { p };
        let x_shown = if rtl { travel - x } else { x };
        scene.set_style(rail, fixed(travel, rail_height));
        scene.set_transform(
            rail,
            Transform {
                x: at(travel, thumb_width / 2.),
                y: (height - rail_height) / 2.,
            },
        );
        scene.set_style(
            fill_viewport,
            Style {
                clip: true,
                ..fixed(travel, rail_height)
            },
        );
        scene.set_style(fill, fixed(travel, rail_height));
        // The fill, as wide as its clip, slides in from the start: its
        // translation inside the clip is the same either way (the clip lays
        // it in at its right, where it already is).
        scene.set_transform(
            fill,
            Transform {
                x: if rtl { travel - x } else { x - travel },
                y: 0.,
            },
        );
        scene.set_transform(
            fill_viewport,
            Transform {
                x: at(travel, thumb_width / 2.),
                y: (height - rail_height) / 2.,
            },
        );
        scene.set_style(thumb, fixed(thumb_width, thumb_height));
        scene.set_transform(
            thumb,
            Transform {
                x: at(thumb_width, x_shown),
                y: (height - thumb_height) / 2.,
            },
        );
        scene.set_kind(
            thumb,
            NodeKind::Rect(typography.get().color_in(Some(&thumb_theme))),
        );
        semantics
            .borrow_mut()
            .update(root, |node| node.numeric_value = Some(val as f64));
    });
    let scene = ui.scene.clone();
    let mut dragging = false;
    ui.on_event(root, true, move |cx| {
        if cx.phase == EventPhase::Capture {
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
        let rtl = scene.borrow().direction(root) == crate::scene::Direction::Rtl;
        let at_position = |x: f32| {
            let scene = scene.borrow();
            let bounds = scene.bounds(rail);
            if bounds.width <= 0. {
                return min as f32;
            }
            let from_start = if rtl {
                bounds.x as f64 + bounds.width as f64 - x as f64
            } else {
                x as f64 - bounds.x as f64
            };
            let fraction = (from_start / bounds.width as f64).clamp(0., 1.);
            let v = min + fraction * span;
            // On a step, counting from the start.
            let v = match step_size {
                Some(s) => min + ((v - min) / s).round() * s,
                None => v,
            };
            v.clamp(min, max) as f32
        };
        // Left and right go the way it shows (right to left, Left is more).
        let toward_right = if rtl { -1. } else { 1. };
        let unit = step_size.unwrap_or(span / 100.);
        let big = matches!(&cx.event, InputEvent::KeyDown { modifiers, .. } if modifiers.shift);
        let step = |direction: f64| {
            let by = if big { unit * 10. } else { unit };
            (value.get() as f64 + direction * by).clamp(min, max) as f32
        };
        let page =
            |direction: f64| (value.get() as f64 + direction * unit * 10.).clamp(min, max) as f32;
        let next = match &cx.event {
            InputEvent::SetNumericValue(v) if v.is_finite() => Some(v.clamp(min, max) as f32),
            InputEvent::SetValue(v) => v
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite())
                .map(|v| v.clamp(min, max) as f32),
            InputEvent::Increment => Some(step(1.)),
            InputEvent::Decrement => Some(step(-1.)),
            InputEvent::KeyDown { key, .. } => match key {
                Key::ArrowLeft => Some(step(-toward_right)),
                Key::ArrowRight => Some(step(toward_right)),
                Key::ArrowDown => Some(step(-1.)),
                Key::ArrowUp => Some(step(1.)),
                Key::PageUp => Some(page(1.)),
                Key::PageDown => Some(page(-1.)),
                Key::Home => Some(min as f32),
                Key::End => Some(max as f32),
                _ => None,
            },
            InputEvent::PointerDown {
                x,
                button: PointerButton::Primary,
                ..
            } => {
                let x = *x;
                dragging = true;
                cx.focus();
                cx.capture_pointer();
                Some(at_position(x))
            }
            InputEvent::PointerMove { x, .. } if dragging => Some(at_position(*x)),
            InputEvent::PointerUp {
                button: crate::input::PointerButton::Primary,
                ..
            }
            | InputEvent::PointerCancel
            | InputEvent::Blur => {
                dragging = false;
                cx.release_pointer();
                None
            }
            _ => None,
        };
        if let Some(next) = next {
            if next.is_finite() {
                value.set(next);
            }
            cx.prevent_default();
            cx.stop_propagation();
        }
    });
}
/// What a view says to assistive technology beyond its name (`View::role`,
/// `heading`, `selected_when`, `live`, `accessible_description`,
/// `accessible_hidden`), boxed: most views say none of it.
/// A number control's value, followed (`View::numeric_when`).
type NumericValue = Box<dyn FnMut() -> f64>;
#[derive(Default)]
pub(crate) struct Accessibility {
    role: Option<Role>,
    level: Option<usize>,
    selected: Option<Box<dyn FnMut() -> bool>>,
    checked: Option<Box<dyn FnMut() -> bool>>,
    radio_item: bool,
    expanded: Option<Box<dyn FnMut() -> bool>>,
    value: Option<Box<dyn FnMut() -> String>>,
    numeric: Option<(f64, f64, NumericValue)>,
    live: Option<Live>,
    description: Option<String>,
    error: Option<Box<dyn FnMut() -> Option<String>>>,
    hidden: bool,
    focus_group: Option<crate::input::FocusGroup>,
    label: Option<Box<dyn FnMut() -> String>>,
}
/// A menu item's text: `label`, or where its label changes
/// (`accessible_label_when`: a translation, a state) what that says now, as
/// its name does. Out of line: `mount_element`'s loop builds few menus, and
/// inlined there it costs every element (measured: +0.6% of it).
#[inline(never)]
fn menu_text(label: String, rare: Option<&mut Rare>) -> View {
    match followed_label(rare) {
        Some(followed) => text_signal(move || followed.get()),
        None => text(label),
    }
}
/// The label a view follows (`accessible_label_when`), shared: what draws it
/// reads it too, the view's name reading it still.
fn followed_label(rare: Option<&mut Rare>) -> Option<Followed> {
    let accessibility = rare?.accessibility.as_mut()?;
    let label = Followed(Rc::new(RefCell::new(accessibility.label.take()?)));
    let read = label.clone();
    accessibility.label = Some(Box::new(move || read.get()));
    Some(label)
}
/// `accessibility` on `root`, which is a `default` (a group, a picture)
/// unless it gives a role.
fn apply_accessibility(ui: &mut Ui, root: NodeId, accessibility: Accessibility, default: Role) {
    let Accessibility {
        role,
        level,
        selected,
        checked,
        radio_item,
        expanded,
        value,
        numeric,
        live,
        description,
        error,
        hidden,
        focus_group,
        label,
    } = accessibility;
    // Composite widgets keep one Tab stop, as a browser's do.
    let focus_group = focus_group.or(match role {
        Some(Role::ListBox) => Some(crate::input::FocusGroup::Vertical),
        Some(Role::TabList | Role::Toolbar) => Some(crate::input::FocusGroup::Horizontal),
        // Every arrow, as a browser's radio buttons take them.
        Some(Role::RadioGroup) => Some(crate::input::FocusGroup::Both),
        _ => None,
    });
    if let Some(axis) = focus_group {
        ui.input.set_focus_group(root, Some(axis));
        // A radio group's arrows choose the radio they move to.
        ui.input
            .set_group_chooses(root, role == Some(Role::RadioGroup));
        // Its children mounted first: one already selected is its stop.
        let selected = {
            let scene = ui.scene.borrow();
            let semantics = ui.semantics.borrow();
            let mut pending = scene.children(root).to_vec();
            let mut found = None;
            while let Some(id) = pending.pop() {
                if semantics.get(id).is_some_and(|n| {
                    n.selected == Some(true)
                        || (n.role == Role::RadioButton && n.checked == Some(true))
                }) {
                    found = Some(id);
                    break;
                }
                pending.extend(scene.children(id).iter().copied());
            }
            found
        };
        if let Some(member) = selected {
            ui.input.set_group_current(&ui.scene.borrow(), member);
        }
    }
    if role == Some(Role::ListBox) {
        crate::typeahead::mount_list_box(ui, root);
    }
    {
        let mut semantics = ui.semantics.borrow_mut();
        if semantics.get(root).is_none() {
            semantics.set(root, SemanticNode::new(role.unwrap_or(default), ""));
        }
        semantics.update(root, |node| {
            if let Some(role) = role {
                node.role = role;
            }
            node.level = level.or(node.level);
            node.live = live.or(node.live);
            node.description = description.or(node.description.take());
            node.hidden |= hidden;
        });
    }
    // A name or value worked out from state is read by assistive technology
    // alone: worked out while one listens, its effect following nothing but
    // that while none does (`Ui::set_assistive_technology`).
    let assistive = ui.assistive();
    if let Some(mut label) = label {
        let tree = ui.semantics.clone();
        tree.borrow_mut().update(root, |node| node.named = true);
        let assistive = assistive.clone();
        ui.bind(root, move || {
            if !assistive.get() {
                return;
            }
            let label = label();
            tree.borrow_mut().update(root, |node| node.label = label);
        });
    }
    if let Some(mut checked) = checked {
        let tree = ui.semantics.clone();
        let weak = ui.downgrade();
        ui.bind(root, move || {
            let checked = checked();
            let mut radio = false;
            tree.borrow_mut().update(root, |node| {
                node.checked = Some(checked);
                node.radio = radio_item;
                radio = node.role == Role::RadioButton;
            });
            if radio
                && checked
                && let Some(ui) = weak.upgrade()
            {
                ui.input.set_group_current(&ui.scene.borrow(), root);
            }
        });
    }
    if let Some(mut error) = error {
        let tree = ui.semantics.clone();
        let assistive = assistive.clone();
        ui.bind(root, move || {
            if !assistive.get() {
                return;
            }
            let error = error();
            tree.borrow_mut().set_error(root, error.as_deref());
        });
    }
    if let Some(mut value) = value {
        let tree = ui.semantics.clone();
        ui.bind(root, move || {
            if !assistive.get() {
                return;
            }
            let value = value();
            tree.borrow_mut()
                .update(root, |node| node.value = Some(value));
        });
    }
    if let Some(mut expanded) = expanded {
        let tree = ui.semantics.clone();
        ui.bind(root, move || {
            let expanded = expanded();
            tree.borrow_mut()
                .update(root, |node| node.expanded = Some(expanded));
        });
    }
    if let Some((min, max, mut value)) = numeric {
        let tree = ui.semantics.clone();
        ui.bind(root, move || {
            let value = value();
            tree.borrow_mut().update(root, |node| {
                node.min = Some(min);
                node.max = Some(max);
                node.numeric_value = Some(value);
            });
        });
    }
    if let Some(mut selected) = selected {
        let tree = ui.semantics.clone();
        let weak = ui.downgrade();
        ui.bind(root, move || {
            let selected = selected();
            tree.borrow_mut()
                .update(root, |node| node.selected = Some(selected));
            // The selected one is where Tab lands in its group.
            if selected && let Some(ui) = weak.upgrade() {
                ui.input.set_group_current(&ui.scene.borrow(), root);
            }
        });
    }
}
/// The labels of the buttons (headings, tabs…) named by their content that
/// hold `node`, as its text changed or it mounted. An element still
/// mounting names itself as it closes, from all it holds by then: the walk
/// ends there, so mounting a tree walks each element once, not each of its
/// ancestors.
fn update_button_labels(ui: &Ui, node: NodeId) {
    let scene = ui.scene.borrow();
    let mounting = ui.innermost_open_element();
    // Most ancestors aren't named by their content: asked in one borrow,
    // let go only to name one.
    let mut semantics = ui.semantics.borrow();
    let mut current = Some(node);
    while let Some(id) = current {
        if Some(id) == mounting {
            break;
        }
        #[cfg(test)]
        LABEL_STEPS.with(|steps| steps.set(steps.get() + 1));
        if named_by_content(&semantics, id) {
            drop(semantics);
            name_content(ui, &scene, id);
            semantics = ui.semantics.borrow();
        }
        current = scene.parent(id);
    }
}
/// Text node `id`'s label from its text, and the buttons it names, as its
/// text binding sets them: deferred while no assistive technology reads
/// the semantics (`Ui::defer_label`). A node removed since is skipped.
pub(crate) fn label_from_text(ui: &Ui, id: NodeId) {
    let text = {
        let scene = ui.scene.borrow();
        if !scene.contains(id) {
            return;
        }
        match scene.kind(id) {
            NodeKind::Text { text, .. } => text.to_string(),
            _ => return,
        }
    };
    ui.semantics.borrow_mut().set_label(id, &text);
    update_button_labels(ui, id);
}
/// Whether `id`'s content names it, as ARIA names a button, heading, tab,
/// option, radio, check box, switch or link from its text (a button given
/// one of those roles is still named by what it holds). Menu items are
/// named by their menus, past their arrows and shortcuts.
fn named_by_content(semantics: &crate::semantics::Semantics, id: NodeId) -> bool {
    semantics.get(id).is_some_and(|n| {
        !n.named
            && matches!(
                n.role,
                Role::Button
                    | Role::Heading
                    | Role::ListItem
                    | Role::ListBoxOption
                    | Role::Tab
                    | Role::RadioButton
                    | Role::CheckBox
                    | Role::Switch
                    | Role::Link
            )
    })
}
/// `id`'s label from its text, if its content names it (`named_by_content`).
fn name_from_content(ui: &Ui, scene: &crate::scene::Scene, id: NodeId) {
    if named_by_content(&ui.semantics.borrow(), id) {
        name_content(ui, scene, id);
    }
}
/// `id`'s label from its text, its content naming it.
fn name_content(ui: &Ui, scene: &crate::scene::Scene, id: NodeId) {
    let mut parts = Vec::new();
    let mut pending = vec![id];
    let semantics = ui.semantics.borrow();
    while let Some(child) = pending.pop() {
        // What is hidden from assistive technology names nothing.
        if child != id && semantics.get(child).is_some_and(|n| n.hidden) {
            continue;
        }
        match scene.kind(child) {
            NodeKind::Text { text, .. } if !text.is_empty() => parts.push(text.as_ref()),
            NodeKind::RichText { text } if !text.text().is_empty() => parts.push(text.text()),
            _ => {}
        }
        pending.extend(scene.children(child).iter().rev().copied());
    }
    drop(semantics);
    let label = parts.join(" ");
    ui.semantics.borrow_mut().set_label(id, &label);
}
/// A mounted node, from `View::on_mount`. Weak: it keeps neither the UI
/// nor the node alive.
#[derive(Clone)]
pub struct NodeHandle {
    ui: WeakUi,
    node: NodeId,
}
impl NodeHandle {
    /// The mounted node.
    pub fn node(&self) -> NodeId {
        self.node
    }
    /// The `Ui` it is mounted in.
    pub(crate) fn ui_handle(&self) -> &WeakUi {
        &self.ui
    }
    pub(crate) fn weak_ui(&self) -> WeakUi {
        self.ui.clone()
    }
    /// Change what assistive technology is told of the node (its
    /// `SemanticNode`) in place: what a component works out itself, as a
    /// grid's current cell (`active_descendant`) or a row's place in a
    /// virtual set. Whether the node has semantics to change.
    pub fn update_semantics(&self, f: impl FnOnce(&mut crate::semantics::SemanticNode)) -> bool {
        let Some(ui) = self.ui.upgrade() else {
            return false;
        };
        let mut semantics = ui.semantics.borrow_mut();
        if semantics.get(self.node).is_none() {
            return false;
        }
        semantics.update(self.node, f);
        true
    }
    /// Whether a text field holds keyboard focus (anywhere in this UI): a
    /// bare-key shortcut (`j`, `k`) gives way to typing then.
    pub fn editing(&self) -> bool {
        self.ui.upgrade().is_some_and(|ui| {
            ui.input
                .focused()
                .is_some_and(|focused| ui.is_editor(focused))
        })
    }
    /// The scene, while the UI exists.
    pub fn scene(&self) -> Option<Rc<RefCell<crate::scene::Scene>>> {
        self.ui.upgrade().map(|ui| ui.scene.clone())
    }
    /// Whether the node is still in the scene.
    pub fn is_mounted(&self) -> bool {
        self.ui
            .upgrade()
            .is_some_and(|ui| ui.scene.borrow().contains(self.node))
    }
    /// The node's world bounds, republished after each frame's layout: for
    /// animating a container to the size of what it holds.
    pub fn observe_bounds(&self) -> Option<Signal<crate::scene::Rect>> {
        let ui = self.ui.upgrade()?;
        ui.scene
            .borrow()
            .contains(self.node)
            .then(|| ui.observe_bounds(self.node))
    }
    /// The node's laid-out size (its border box), republished after each
    /// layout that changes it: for what depends on a size alone (a
    /// ResizeObserver), at no cost when only something above it moves.
    pub fn observe_size(&self) -> Option<Signal<(f32, f32)>> {
        let ui = self.ui.upgrade()?;
        ui.scene
            .borrow()
            .contains(self.node)
            .then(|| ui.observe_size(self.node))
    }
    /// Whether any of the node shows (inside every clip and viewport above
    /// it), republished after each frame's layout: for building what only
    /// matters on screen.
    pub fn observe_visibility(&self) -> Option<Signal<bool>> {
        let ui = self.ui.upgrade()?;
        ui.scene
            .borrow()
            .contains(self.node)
            .then(|| ui.observe_visibility(self.node))
    }
    /// Sweep the node's mask (`Styled::mask`) from where its style puts it
    /// by `by`, over and over every `period`, at `max_rate` steps a second
    /// at most (a text shimmer's bright band). The scene moves the mask
    /// itself on the window's frame clock: no style pass, layout or script
    /// runs, and only the masked node composites again. While none of the
    /// node shows it steps 4 times a second; it ends when the node goes.
    /// Spawn it from the component's tasks.
    pub fn sweep_mask(
        &self,
        frames: crate::frame::FrameClock,
        by: (f32, f32),
        period: std::time::Duration,
        max_rate: f64,
    ) -> impl std::future::Future<Output = ()> + 'static {
        let (ui, node) = (self.ui.clone(), self.node);
        async move {
            let period = period.as_secs_f64().max(1e-3);
            let start = frames.next().max_rate(max_rate).await.time;
            let mut from = None;
            let mut shown = true;
            loop {
                let frame = frames
                    .next()
                    // Unseen, it only looks for showing again.
                    .max_rate(if shown { max_rate } else { 4. })
                    .await;
                let Some(ui) = ui.upgrade() else { return };
                let mut scene = ui.scene.borrow_mut();
                if !scene.contains(node) {
                    return;
                }
                shown = scene.visible_bounds(node).is_some();
                let Some(mut mask) = scene.mask(node).cloned() else {
                    continue;
                };
                // The style's offset is where each sweep starts; a restyle
                // puts it back there.
                let origin = *from.get_or_insert(mask.offset);
                let t =
                    (frame.time.saturating_duration_since(start).as_secs_f64() % period) / period;
                mask.offset = (origin.0 + by.0 * t as f32, origin.1 + by.1 * t as f32);
                scene.set_mask(node, Some(mask));
            }
        }
    }
    /// Run `f` once after the next frame's layout, before that frame is
    /// drawn, whether or not anything moved: for reading where a change just
    /// put things (and setting a translation to start a move from where they
    /// were). It runs even if the node has gone by then.
    pub fn after_layout(&self, f: impl FnOnce() + 'static) {
        if let Some(ui) = self.ui.upgrade() {
            ui.after_layout(Box::new(f));
        }
    }
    /// Run `f` after every layout pass that changes the layout (not for
    /// frames that only translate, fade or scroll), until the node is removed.
    pub fn on_layout(&self, f: impl FnMut() + 'static) {
        if let Some(ui) = self.ui.upgrade()
            && ui.scene.borrow().contains(self.node)
        {
            ui.on_layout(self.node, Box::new(f));
        }
    }
    /// The editor this node is (an input or a text area), if it is one.
    pub fn editor(&self) -> Option<crate::widgets::EditorHandle> {
        let ui = self.ui.upgrade()?;
        ui.editor(self.node)
    }
    /// An editor's selection, `(anchor, focus)` in UTF-8 byte offsets
    /// (see `EditorHandle::selection`); `None` for other nodes.
    pub fn selection(&self) -> Option<(usize, usize)> {
        self.editor().map(|editor| editor.selection())
    }
    /// Select an editor's text from `anchor` to `focus`, UTF-8 byte offsets
    /// (see `EditorHandle::set_selection`); whether the node is an editor.
    pub fn set_selection(&self, anchor: usize, focus: usize) -> bool {
        self.editor()
            .map(|editor| editor.set_selection(anchor, focus))
            .is_some()
    }
    /// Select an editor's whole text; whether the node is an editor.
    pub fn select_all(&self) -> bool {
        self.editor().map(|editor| editor.select_all()).is_some()
    }
    /// A picture of the node as the window last showed it, at most
    /// `max_width` pixels wide (see `Ui::snapshot`).
    pub fn snapshot(&self, max_width: u32) -> Option<std::sync::Arc<crate::image::ImageData>> {
        self.ui.upgrade()?.snapshot(self.node, max_width)
    }
    /// Keep `resource` until the node is removed, then drop it.
    pub fn retain(&self, resource: impl Any) {
        if let Some(mut ui) = self.ui.upgrade()
            && ui.scene.borrow().contains(self.node)
        {
            ui.retain(self.node, resource);
        }
    }
    /// Scroll the virtual list this is to row `index` (from 0), placed as
    /// `align` says, mounting it: a list of measured rows measures the rows
    /// around it as they land, so it ends where asked. False if this is not
    /// a mounted virtual list.
    pub fn scroll_to_row(&self, index: usize, align: RowAlign) -> bool {
        let Some(ui) = self.ui.upgrade() else {
            return false;
        };
        let reveal = ui.list_reveals().borrow().get(&self.node).cloned();
        let Some(reveal) = reveal else {
            return false;
        };
        reveal.update(|(serial, row, how)| {
            *serial = serial.wrapping_add(1);
            *row = index;
            *how = align;
        });
        true
    }
    /// Move keyboard focus to the node, or to the first focusable node inside
    /// it (a field's editor). False when nothing there can take focus.
    pub fn focus(&self) -> bool {
        let Some(ui) = self.ui.upgrade() else {
            return false;
        };
        let mut pending = vec![self.node];
        while let Some(node) = pending.pop() {
            // Hidden content takes no focus (an input in a closed panel).
            if !ui.scene.borrow().contains(node) || !ui.scene.borrow().layout_visible(node) {
                continue;
            }
            if ui.input.focused() == Some(node) || ui.input.focus(&ui.scene, Some(node)) {
                return true;
            }
            pending.extend(ui.scene.borrow().children(node).iter().rev().copied());
        }
        false
    }
    /// Clear keyboard focus when it is on the node or inside it.
    pub fn blur(&self) {
        let Some(ui) = self.ui.upgrade() else {
            return;
        };
        let inside = ui.input.focused().is_some_and(|focused| {
            let scene = ui.scene.borrow();
            focused == self.node
                || (scene.contains(focused) && scene.ancestors(focused).any(|n| n == self.node))
        });
        if inside {
            ui.input.focus(&ui.scene, None);
        }
    }
}
/// Reference to a Ui-owned component tree. Drop does not unmount; call `unmount`.
#[derive(Clone)]
pub struct ViewHandle {
    ui: WeakUi,
    root: NodeId,
    registry: Registry,
}
impl ViewHandle {
    /// The tree's root node.
    pub fn node(&self) -> NodeId {
        self.root
    }
    /// Whether the tree is still in the scene.
    pub fn is_mounted(&self) -> bool {
        self.ui
            .upgrade()
            .is_some_and(|ui| ui.scene.borrow().contains(self.root))
    }
    /// Remove the tree from the scene, disposing its state and handlers.
    pub fn unmount(&self) {
        if let Some(mut ui) = self.ui.upgrade() {
            ui.remove(self.root);
        }
    }
    /// The mounted node named `id` (`View::id`) in this tree, if any.
    pub fn find(&self, id: &str) -> Option<NodeId> {
        let ui = self.ui.upgrade()?;
        let registry = self.registry.borrow();
        registry
            .get(id)?
            .iter()
            .copied()
            .find(|node| ui.scene.borrow().contains(*node))
    }
    /// Every mounted node named `id` in this tree (rows of a list may share
    /// one), in the order they were made.
    pub fn find_all(&self, id: &str) -> Vec<NodeId> {
        let Some(ui) = self.ui.upgrade() else {
            return Vec::new();
        };
        let registry = self.registry.borrow();
        let scene = ui.scene.borrow();
        registry
            .get(id)
            .map(|nodes| {
                nodes
                    .iter()
                    .copied()
                    .filter(|n| scene.contains(*n))
                    .collect()
            })
            .unwrap_or_default()
    }
    /// Every mounted node with an id in this tree, and its id.
    pub fn ids(&self) -> Vec<(NodeId, String)> {
        let Some(ui) = self.ui.upgrade() else {
            return Vec::new();
        };
        let registry = self.registry.borrow();
        let scene = ui.scene.borrow();
        let mut out: Vec<(NodeId, String)> = registry
            .iter()
            .flat_map(|(id, nodes)| {
                nodes
                    .iter()
                    .filter(|n| scene.contains(**n))
                    .map(move |n| (*n, id.clone()))
            })
            .collect();
        out.sort_by_key(|(n, _)| n.number());
        out
    }
}
impl Ui {
    /// Mount a declarative tree under the document root. The document owns its lifetime.
    pub fn mount(&mut self, view: View) -> ViewHandle {
        let registry = Registry::default();
        let theme = self.theme().root().clone();
        // Text shows the theme's body colour, size and family unless a view
        // says otherwise; each text node reads them from its own scope.
        let typography = self.signal(Typography {
            display: Default::default(),
            color: crate::theme::Themed::Token(tok::TEXT_PRIMARY.into()),
            size: crate::theme::Themed::Token(tok::FONT_SIZE_MD.into()),
            wrap: false,
            font: Default::default(),
            family: Some(tok::FONT_SANS.into()),
        });
        let environment = Environment {
            services: ServiceScope::new(),
            typography,
            registry: registry.clone(),
            portal: None,
            menu: None,
            measure: self.scene.borrow().text_measure(),
            theme,
        };
        let runtime = self.runtime.clone();
        let mut ui = self.shared();
        // Initial model normalization can notify observers that remove the new
        // subtree. Attach the whole tree and its ownership before they run.
        let mut transaction = MountTransaction::new(&ui, self.root());
        let initialization = ui.begin_mount_initialization();
        // Text inputs are drawn once the bindings that read no layout have
        // run, not each as it binds (`InputDraws`).
        let draws = crate::widgets::InputDrawScope::begin(self);
        let build = || {
            let _scope = initialization.enter();
            let root = runtime.untracked(|| mount(&mut ui, self.root(), view, environment));
            initialization.set_root(root);
            root
        };
        let root = if draws.active {
            let root = runtime.batch_watched(
                build,
                |effect, starts| crate::widgets::watch_geometry(effect, starts, &self.scene),
                crate::widgets::inputs_bound,
            );
            crate::widgets::finish_input_draws(&self.scene);
            root
        } else {
            runtime.batch(build)
        };
        transaction.commit();
        ViewHandle {
            ui: self.downgrade(),
            root,
            registry,
        }
    }
    /// Replace root content explicitly. Ordinary property changes do not call this.
    pub fn render(&mut self, view: View) -> ViewHandle {
        let children = self.scene.borrow().children(self.root()).to_vec();
        let mounted = self.mount(view);
        for child in children {
            self.remove(child);
        }
        mounted
    }
}
pub use crate::compose_split::{SplitOptions, split};

/// Common imports for component application code.
pub mod prelude {
    pub use super::{
        Context, Slot, VariableHeights, View, button, checkbox, column, component, div, image,
        image_signal, keyed, measured_rows, measured_virtual_list, menu, menu_item, modal, overlay,
        popover, progress, provide, provide_with, radio, radio_group, row, scroll, scroll_x,
        scrollbar, slider, submenu, switch, text, text_area, text_area_value, text_input,
        text_input_value, text_message, text_signal, variable_virtual_list, virtual_list,
    };
    pub use super::{RichTextView, TextSpan, rich_text, rich_text_signal, text_span};
    pub use super::{SplitOptions, split};
    pub use super::{animated_image, animated_image_controlled, async_image, canvas, gpu_view};
    #[cfg(target_os = "macos")]
    pub use super::{native_surface, native_surface_signal};
    pub use super::{svg, svg_signal};
    pub use super::{theme_scope, theme_scope_with};
    pub use crate::affine::Affine;
    pub use crate::canvas::{
        Brush, Canvas, FillRule, GradientStop, LineCap, LineJoin, Path, PathBuilder, Point, Stroke,
    };
    pub use crate::cursor::Cursor;
    pub use crate::decoration::{Background, BorderStyle, Corners};
    pub use crate::rich_text::Decoration;
    pub use crate::style::{ObjectFit, StrokeAlign, Styled, Styles, rgb, rgba};
    pub use crate::svg::SvgData;
    pub use crate::text_layout::{FontFamily, FontFeatures, TextAlign, TextOptions, TextOverflow};
    pub use crate::theme::{ThemeOverride, tok};
    pub use crate::web_view::{native_view, web_view, web_view_src};
}

#[cfg(test)]
thread_local! {
    /// Ancestors `update_button_labels` has looked at.
    static LABEL_STEPS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
#[cfg(test)]
mod button_label_tests {
    use super::*;

    #[test]
    fn mounting_a_deep_tree_looks_at_each_element_once_and_names_its_button() {
        let mut ui = Ui::new(400., 300.);
        let mut inner = text("deep");
        for _ in 0..2_000 {
            inner = column().child(inner);
        }
        LABEL_STEPS.with(|steps| steps.set(0));
        let handle = ui.mount(button().id("b").child(text("Save")).child(inner));
        let steps = LABEL_STEPS.with(|steps| steps.get());
        // Each element once, and the outer walk to the root: not each
        // element's ancestors (two million).
        assert!(steps < 3 * 2_000, "{steps}");
        let button = handle.find("b").unwrap();
        assert_eq!(
            ui.semantics.borrow().get(button).unwrap().label,
            "Save deep"
        );
    }
}
#[cfg(test)]
mod environment_tests {
    #[test]
    fn an_environment_stays_small() {
        // Cloned for every component and region mounted: growing it costs
        // every mount (a flag beside its portal took 8 bytes).
        assert_eq!(std::mem::size_of::<super::Environment>(), 112);
    }
}
#[cfg(test)]
mod image_fit_tests {
    use super::fitted_image_size;
    use crate::style::ObjectFit;

    #[test]
    fn extreme_aspect_scaling_remains_finite_and_zero_content_has_no_bitmap() {
        for mode in [
            ObjectFit::Fill,
            ObjectFit::Contain,
            ObjectFit::Cover,
            ObjectFit::None,
            ObjectFit::ScaleDown,
        ] {
            let (width, height) = fitted_image_size(f32::MAX, 1., 1., u32::MAX as f32, mode);
            assert!(width.is_finite() && height.is_finite());
            assert!(width >= 0. && height >= 0.);
            assert_eq!(fitted_image_size(0., 100., 20., 10., mode), (0., 0.));
            assert_eq!(fitted_image_size(100., 0., 20., 10., mode), (0., 0.));
        }
    }
}

#[cfg(test)]
mod editor_mount_tests {
    use super::*;

    #[test]
    fn canonical_writeback_can_unmount_a_fully_registered_declarative_tree() {
        let mut ui = Ui::new(400., 300.);
        let model = ui.signal("a\r\nb".to_owned());
        let observed = model.clone();
        let weak = ui.downgrade();
        let watcher = ui.runtime.effect(move || {
            if observed.get() == "ab" {
                let mut ui = weak.upgrade().unwrap();
                let children = ui.scene.borrow().children(ui.root()).to_vec();
                for child in children {
                    ui.remove(child);
                }
            }
        });
        let view = ui.mount(
            column()
                .child(text_input("Editor", model.clone()))
                .child(text("After")),
        );
        assert!(!ui.scene.borrow().contains(view.node()));
        assert_eq!(ui.runtime.effect_count(), 1);
        model.set("next\nvalue".to_owned());
        ui.prepare_frame();
        assert_eq!(model.get(), "next\nvalue");
        assert!(ui.scene.borrow().children(ui.root()).is_empty());
        drop(watcher);
        assert_eq!(ui.runtime.effect_count(), 0);
    }
}

#[cfg(test)]
mod input_draw_tests {
    use super::*;
    use std::{cell::Cell, rc::Rc};

    // Drawn as it bound, the input was last laid out before its width
    // applied: the callback read the default 240, and a style derived from
    // it stayed 240. Its geometry read is reactive now: once the mount's
    // layout settles, the callback runs again and reads 160.
    #[test]
    fn a_mount_callback_derives_its_style_from_current_input_geometry() {
        let mut ui = Ui::new(1200., 800.);
        let value = ui.signal(String::from("abcdefghijabcdefghijabcdefghij"));
        let scene = ui.scene.clone();
        let observed = Rc::new(Cell::new(0.));
        let read = observed.clone();
        let mounted = ui.mount(
            column().children([
                row().child(text_input("Name", value).id("source").w(160.)),
                button()
                    .id("derived")
                    .child(text("Derived"))
                    .reactive_style(move || {
                        let scene = scene.borrow();
                        let column = scene.children(scene.root())[0];
                        let row = scene.children(column)[0];
                        let input = scene.children(row)[0];
                        assert_eq!(scene.style(input).width, Some(160.));
                        let width = scene.bounds(input).width;
                        read.set(width);
                        Styles::default().w(width)
                    }),
            ]),
        );
        assert_eq!(observed.get(), 160.);
        ui.prepare_frame();
        let derived = mounted.find("derived").unwrap();
        assert_eq!(ui.scene.borrow().bounds(derived).width, 160.);
    }

    // A callback reading geometry during a mount may run again once its
    // layout settles: one that keeps only its first read keeps a stale one.
    #[test]
    fn a_mount_callback_that_latches_its_first_geometry_read_keeps_it() {
        let mut ui = Ui::new(1200., 800.);
        let value = ui.signal(String::from("text"));
        let scene = ui.scene.clone();
        let (runs, latched) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(None)));
        let (count, saved) = (runs.clone(), latched.clone());
        let input = Rc::new(Cell::new(None));
        let (source, read) = (input.clone(), input.clone());
        let _mounted = ui.mount(
            column().children([
                row().child(
                    text_input("Name", value)
                        .w(160.)
                        .on_mount(move |handle| source.set(Some(handle.node()))),
                ),
                button().child(text("Derived")).reactive_style(move || {
                    count.set(count.get() + 1);
                    let width = read
                        .get()
                        .map_or(0., |input| scene.borrow().bounds(input).width);
                    if saved.get().is_none() {
                        saved.set(Some(width));
                    }
                    Styles::default()
                }),
            ]),
        );
        // Run first before layout, then again on current geometry; its
        // latch kept the first.
        assert_eq!(runs.get(), 2);
        assert_ne!(latched.get(), Some(160.));
        let input = input.get().unwrap();
        assert_eq!(ui.scene.borrow().bounds(input).width, 160.);
    }

    // A callback whose read moves again each time it runs is run again a
    // few times, then left for the next flush: never a loop.
    #[test]
    fn a_mount_callback_whose_geometry_never_settles_stops_after_a_few_passes() {
        let mut ui = Ui::new(1200., 800.);
        let value = ui.signal(String::from("text"));
        let scene = ui.scene.clone();
        let runs = Rc::new(Cell::new(0));
        let count = runs.clone();
        let mounted = ui.mount(
            column().children([
                row().child(text_input("Name", value).w(160.)),
                button()
                    .id("grows")
                    .child(text("Grows"))
                    .reactive_style(move || {
                        count.set(count.get() + 1);
                        let scene = scene.borrow();
                        let column = scene.children(scene.root())[0];
                        let grows = scene.children(column)[1];
                        // Wider by a point than it was laid out: never settled.
                        Styles::default().w(scene.bounds(grows).width + 1.)
                    }),
            ]),
        );
        assert!(runs.get() <= crate::widgets::GEOMETRY_PASSES + 1);
        assert!(mounted.find("grows").is_some());
    }

    fn picture(ui: &Ui) -> Vec<String> {
        let scene = ui.scene.borrow();
        scene
            .layer_items(None)
            .iter()
            .map(|item| {
                format!(
                    "{:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?}",
                    item.id,
                    item.bounds,
                    item.kind,
                    item.font,
                    item.text_options,
                    item.effects,
                    item.clip,
                    item.border_pass,
                    item.isolated,
                    item.extras
                )
            })
            .collect()
    }

    fn shaper_read_case(eager: bool) -> Vec<String> {
        let mut ui = Ui::new(600., 400.);
        let metric = ui.signal(1.0f32);
        let read = metric.clone();
        let calls = Rc::new(Cell::new(0));
        let measured = calls.clone();
        ui.scene.borrow_mut().set_text_shaper(
            move |text: &str,
                  size: f32,
                  width: Option<f32>|
                  -> Box<dyn crate::text_layout::TextLayout> {
                measured.set(measured.get() + 1);
                Box::new(crate::text_layout::FallbackTextLayout::new(
                    text,
                    size * read.get(),
                    width,
                ))
            },
        );
        let value = ui.signal(String::from("not empty"));
        let view = column().child(text_input("Input", value).w(160.));
        let runtime = ui.runtime.clone();
        // Inside an outer batch a mount draws each input as it binds.
        let _mounted = if eager {
            runtime.batch(|| ui.mount(view))
        } else {
            ui.mount(view)
        };
        ui.prepare_frame();
        assert!(metric.is_observed());
        let before = calls.get();
        metric.set(2.);
        ui.prepare_frame();
        assert!(calls.get() > before);
        picture(&ui)
    }
    #[test]
    fn a_deferred_initial_draw_keeps_its_custom_shapers_signal_dependency() {
        assert_eq!(shaper_read_case(false), shaper_read_case(true));
    }

    fn shaper_write_case(eager: bool) -> Vec<String> {
        let mut ui = Ui::new(600., 400.);
        let value = ui.signal(String::from("first"));
        let write = value.clone();
        let fired = Rc::new(Cell::new(false));
        let once = fired.clone();
        ui.scene.borrow_mut().set_text_shaper(
            move |text: &str,
                  size: f32,
                  width: Option<f32>|
                  -> Box<dyn crate::text_layout::TextLayout> {
                if text == "first" && !once.replace(true) {
                    write.set(String::from("second"));
                }
                Box::new(crate::text_layout::FallbackTextLayout::new(
                    text, size, width,
                ))
            },
        );
        let view = column().child(text_input("Input", value.clone()).w(160.));
        let runtime = ui.runtime.clone();
        let _mounted = if eager {
            runtime.batch(|| ui.mount(view))
        } else {
            ui.mount(view)
        };
        ui.prepare_frame();
        assert_eq!(value.get(), "second");
        picture(&ui)
    }
    #[test]
    fn a_signal_written_by_a_deferred_shaper_waits_until_the_draw_finishes() {
        assert_eq!(shaper_write_case(false), shaper_write_case(true));
    }
}

#[cfg(test)]
mod keyed_removal_tests {
    use super::*;
    use std::cell::Cell;
    struct ReadChildren {
        scene: Rc<RefCell<crate::scene::Scene>>,
        parent: Rc<Cell<Option<NodeId>>>,
        seen: Rc<RefCell<Vec<Vec<NodeId>>>>,
    }
    impl Drop for ReadChildren {
        fn drop(&mut self) {
            if let Some(parent) = self.parent.get() {
                self.seen
                    .borrow_mut()
                    .push(self.scene.borrow().children(parent).to_vec());
            }
        }
    }
    #[test]
    fn a_removed_rows_resource_reads_its_parents_children_as_they_are() {
        let mut ui = Ui::new(800., 600.);
        let keys = ui.signal((0..8u32).collect::<Vec<_>>());
        let parent = Rc::new(Cell::new(None));
        let seen = Rc::new(RefCell::new(Vec::new()));
        let scene = ui.scene.clone();
        let (read_keys, read_parent, read_seen) = (keys.clone(), parent.clone(), seen.clone());
        let view = ui.mount(keyed(
            move || read_keys.get(),
            move |_, cx| {
                cx.retain(ReadChildren {
                    scene: scene.clone(),
                    parent: read_parent.clone(),
                    seen: read_seen.clone(),
                });
                column()
            },
        ));
        parent.set(Some(view.node()));
        let before = ui.scene.borrow().children(view.node()).to_vec();
        assert_eq!(before.len(), 8);
        keys.set((4..8).collect());
        let snapshots = seen.borrow();
        assert_eq!(snapshots.len(), 4);
        for (at, snapshot) in snapshots.iter().enumerate() {
            assert_eq!(snapshot, &before[at + 1..]);
        }
        assert_eq!(ui.scene.borrow().children(view.node()), &before[4..]);
        parent.set(None);
    }
}
