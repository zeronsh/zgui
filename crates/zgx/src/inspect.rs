//! Developer diagnostics: the layout inspector and layout warnings.
//!
//! The inspector outlines every node's box and describes the one under the
//! pointer: its kind, the label a front end gave it (a Luau script's
//! `file:line`, its `id` and classes), size, position, padding, margin,
//! border, flex properties and clip. It is drawn as an overlay of ordinary
//! scene nodes appended last under the scene root: absolutely positioned
//! with a fixed size, so it takes no room in the interface's layout and its
//! own changes lay out only itself; inert (`Scene::set_inert`), so hit
//! testing, input and cursors pass through it to the interface.
//!
//! Layout warnings report suspicious results after layout, once per node, to
//! stderr and the inspector: a zero-width or zero-height box with content,
//! content overflowing a clipping box that does not scroll, and a percentage
//! size that could not resolve because its parent sizes to its content.
//!
//! Off (the default), an [`Inspector`] costs nothing per frame: hosts check
//! [`Inspector::active`], one flag, and it holds no nodes. Hosts turn it on
//! with [`Inspector::set_enabled`]; `ZGX_INSPECT=1` turns it on and
//! `ZGX_LAYOUT_WARNINGS=1` turns on the warnings alone. The desktop host
//! toggles it with Ctrl+Option+Cmd+I (Ctrl+Alt+Shift+I elsewhere).
use crate::{
    layout::Length,
    scene::{
        Color, Insets, Layout, NodeId, NodeKind, QuadStyle, Rect, Scene, Style, Transform, WalkItem,
    },
    text_layout::{FontFamily, FontStyle},
    widgets::Ui,
};
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    fmt,
    rc::Rc,
};

/// What a front end knows about a node it created: a description (a
/// script's element kind, `#id` and `.class`es) and where it was created.
#[derive(Clone, Debug, PartialEq)]
pub struct Label {
    /// Its description: the element kind, `#id` and `.class`es.
    pub name: Rc<str>,
    /// Where it was created, such as `app.luau:42`.
    pub source: Option<Rc<str>>,
}

/// What a layout [`Warning`] is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WarningKind {
    /// A zero-width or zero-height box with children or text.
    ZeroSize,
    /// In-flow content past the edges of a clipping box that does not scroll.
    Overflow,
    /// A percentage width or height inside a parent sized by its content.
    UnresolvedPercent,
}

/// A suspicious layout result.
#[derive(Clone, Debug, PartialEq)]
pub struct Warning {
    /// The node warned about.
    pub node: NodeId,
    /// What kind of problem it is.
    pub kind: WarningKind,
    /// The node, as the inspector names it (`column #list`).
    pub subject: String,
    /// The nearest creator's location (`app.luau:42`), when known.
    pub source: Option<Rc<str>>,
    /// What is wrong, with a hint to fix it.
    pub message: String,
}
impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(source) = &self.source {
            write!(f, "{source}: ")?;
        }
        write!(f, "{}: {}", self.subject, self.message)
    }
}

/// One window's inspector. Cloning shares it.
#[derive(Clone)]
pub struct Inspector(Rc<Inner>);
struct Inner {
    enabled: Cell<bool>,
    warnings: Cell<bool>,
    /// Whether hosts must call `update`: enabled, warning, or an overlay
    /// still to remove.
    active: Cell<bool>,
    labels: RefCell<HashMap<NodeId, Label>>,
    hooks: RefCell<Vec<Hook>>,
    wake: RefCell<Option<Rc<dyn Fn()>>>,
    state: RefCell<State>,
}
type Hook = Rc<dyn Fn(bool)>;
/// Layout and geometry revisions, the pointer and the viewport.
type Built = (u64, u64, Option<(f32, f32)>, Rect);
#[derive(Default)]
struct State {
    overlay: Option<Overlay>,
    /// What the overlay was last built from: layout and geometry revisions,
    /// the pointer and the viewport.
    built: Option<Built>,
    /// The layout revision warnings were last checked at.
    checked: Option<u64>,
    reported: HashSet<(NodeId, WarningKind)>,
    warnings: Vec<Warning>,
    details: Vec<Line>,
    picked: Option<NodeId>,
}
struct Overlay {
    root: NodeId,
    outlines: Vec<NodeId>,
    /// The picked node's margin, border and content boxes.
    marks: [NodeId; 3],
    panel: NodeId,
    lines: Vec<NodeId>,
}
#[derive(Clone, Debug, PartialEq)]
struct Line {
    text: String,
    tone: Tone,
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Tone {
    Title,
    Subject,
    Plain,
    Dim,
    Source,
    Warning,
}

/// Outlines drawn at most: the inspector stays usable in huge scenes.
const MAX_OUTLINES: usize = 4000;
const OUTLINE: Color = Color(80, 170, 255, 120);
const WARNED: Color = Color(255, 140, 40, 235);
const PICKED_FILL: Color = Color(80, 160, 255, 46);
const PICKED_BORDER: Color = Color(110, 185, 255, 255);
const CONTENT: Color = Color(120, 220, 140, 210);
const MARGIN: Color = Color(255, 190, 90, 170);

fn env_on(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|v| !v.is_empty() && v != "0")
}

impl Default for Inspector {
    fn default() -> Self {
        Self::new()
    }
}
impl Inspector {
    /// An inspector configured from the environment: `ZGX_INSPECT=1` turns
    /// it on, `ZGX_LAYOUT_WARNINGS=1` turns on the warnings alone.
    pub fn new() -> Self {
        let inspector = Self::off();
        inspector.set_warnings(env_on("ZGX_LAYOUT_WARNINGS"));
        inspector.set_enabled(env_on("ZGX_INSPECT"));
        inspector
    }
    /// An inspector that is off, whatever the environment says.
    pub fn off() -> Self {
        Self(Rc::new(Inner {
            enabled: Cell::new(false),
            warnings: Cell::new(false),
            active: Cell::new(false),
            labels: RefCell::default(),
            hooks: RefCell::default(),
            wake: RefCell::default(),
            state: RefCell::default(),
        }))
    }
    /// Whether the host must call [`Inspector::update`] each frame. Off,
    /// this flag is all an inspector costs.
    pub fn active(&self) -> bool {
        self.0.active.get()
    }
    /// Whether the overlay shows.
    pub fn enabled(&self) -> bool {
        self.0.enabled.get()
    }
    /// Shows or hides the overlay. On, layout warnings are checked too.
    pub fn set_enabled(&self, enabled: bool) {
        if self.0.enabled.replace(enabled) == enabled {
            return;
        }
        self.refresh_active();
        self.0.state.borrow_mut().built = None;
        if !self.recording() {
            self.0.labels.borrow_mut().clear();
        }
        let hooks: Vec<_> = self.0.hooks.borrow().clone();
        for hook in hooks {
            hook(self.recording());
        }
        let wake = self.0.wake.borrow().clone();
        if let Some(wake) = wake {
            wake();
        }
    }
    /// Show the overlay if it is hidden, else hide it.
    pub fn toggle(&self) {
        self.set_enabled(!self.enabled());
    }
    /// Whether layout warnings are checked (always, while enabled).
    pub fn warnings_enabled(&self) -> bool {
        self.0.warnings.get() || self.0.enabled.get()
    }
    /// Checks layout warnings after each layout, also while the overlay is
    /// hidden (`ZGX_LAYOUT_WARNINGS=1`).
    pub fn set_warnings(&self, warnings: bool) {
        if self.0.warnings.replace(warnings) == warnings {
            return;
        }
        self.refresh_active();
        let hooks: Vec<_> = self.0.hooks.borrow().clone();
        for hook in hooks {
            hook(self.recording());
        }
    }
    /// Whether front ends should label the nodes they create
    /// ([`Inspector::label`]): while the overlay or the warnings are on.
    pub fn recording(&self) -> bool {
        self.warnings_enabled()
    }
    /// Calls `hook` with [`Inspector::recording`] whenever it may have
    /// changed: a front end that labels nodes only while recording can
    /// rebuild its interface to label what is already mounted.
    pub fn on_recording(&self, hook: impl Fn(bool) + 'static) {
        self.0.hooks.borrow_mut().push(Rc::new(hook));
    }
    /// How the host is asked for a frame when the inspector changes outside
    /// one (turned on from a task).
    pub fn set_wake(&self, wake: impl Fn() + 'static) {
        *self.0.wake.borrow_mut() = Some(Rc::new(wake));
    }
    /// Names `node` for the inspector and its warnings. Ignored unless
    /// recording.
    pub fn label(&self, node: NodeId, name: impl Into<Rc<str>>, source: Option<Rc<str>>) {
        if !self.recording() {
            return;
        }
        self.0.labels.borrow_mut().insert(
            node,
            Label {
                name: name.into(),
                source,
            },
        );
    }
    /// The label of `node` itself.
    pub fn label_of(&self, node: NodeId) -> Option<Label> {
        self.0.labels.borrow().get(&node).cloned()
    }
    /// The warnings found at the last check, in tree order.
    pub fn warnings(&self) -> Vec<Warning> {
        self.0.state.borrow().warnings.clone()
    }
    /// The overlay's root, while it is shown.
    pub fn overlay(&self) -> Option<NodeId> {
        self.0.state.borrow().overlay.as_ref().map(|o| o.root)
    }
    /// The node under the pointer at the last update.
    pub fn picked(&self) -> Option<NodeId> {
        self.0.state.borrow().picked
    }
    /// The details panel's text, one line per entry.
    pub fn details(&self) -> Vec<String> {
        self.0
            .state
            .borrow()
            .details
            .iter()
            .map(|l| l.text.clone())
            .collect()
    }
    /// The boxes outlined at the last update.
    pub fn outlines(&self) -> usize {
        self.0
            .state
            .borrow()
            .overlay
            .as_ref()
            .map_or(0, |o| o.outlines.len())
    }
    fn refresh_active(&self) {
        let overlay = self.0.state.borrow().overlay.is_some();
        self.0
            .active
            .set(self.0.enabled.get() || self.0.warnings.get() || overlay);
    }

    /// After layout, before rendering: checks warnings if layout changed,
    /// and builds or removes the overlay, picking the node under `pointer`
    /// (logical window coordinates; `None` outside the window). Cheap when
    /// nothing changed since the last call. Hosts call it while
    /// [`Inspector::active`].
    pub fn update(&self, ui: &Ui, pointer: Option<(f32, f32)>) {
        if !self.active() {
            return;
        }
        let mut state = self.0.state.borrow_mut();
        if !self.enabled()
            && let Some(overlay) = state.overlay.take()
        {
            let mut scene = ui.scene.borrow_mut();
            if scene.contains(overlay.root) {
                scene.remove(overlay.root);
            }
            state.built = None;
            state.picked = None;
            state.details.clear();
        }
        let layout = ui.scene.borrow().layout_revision();
        if self.warnings_enabled() && state.checked != Some(layout) {
            state.checked = Some(layout);
            self.0
                .labels
                .borrow_mut()
                .retain(|id, _| ui.scene.borrow().contains(*id));
            let warnings = check(ui, &self.0.labels.borrow());
            for warning in &warnings {
                if state.reported.insert((warning.node, warning.kind)) {
                    eprintln!("zgx layout warning: {warning}");
                }
            }
            state
                .reported
                .retain(|(id, _)| ui.scene.borrow().contains(*id));
            state.warnings = warnings;
        }
        if self.enabled() {
            let key = {
                let scene = ui.scene.borrow();
                (
                    layout,
                    scene.geometry_revision(),
                    pointer,
                    scene.bounds(scene.root()),
                )
            };
            if state.built != Some(key) {
                self.build(ui, &mut state, pointer);
                let scene = ui.scene.borrow();
                state.built = Some((
                    scene.layout_revision(),
                    scene.geometry_revision(),
                    pointer,
                    scene.bounds(scene.root()),
                ));
            }
        }
        drop(state);
        self.refresh_active();
    }

    fn build(&self, ui: &Ui, state: &mut State, pointer: Option<(f32, f32)>) {
        let mut scene = ui.scene.borrow_mut();
        let root = scene.root();
        let viewport = scene.bounds(root);
        // The interface may have replaced the root's children (`Ui::render`).
        if state
            .overlay
            .as_ref()
            .is_some_and(|o| !scene.contains(o.root))
        {
            state.overlay = None;
        }
        let overlay = state
            .overlay
            .get_or_insert_with(|| create_overlay(&mut scene));
        // Last under the root, so it paints over everything, portals too.
        let children = scene.children(root).to_vec();
        if children.last() != Some(&overlay.root) {
            let mut order: Vec<_> = children
                .into_iter()
                .filter(|c| *c != overlay.root)
                .collect();
            order.push(overlay.root);
            scene.reorder_children(root, &order);
        }
        scene.set_style(
            overlay.root,
            Style {
                absolute: true,
                width: Some(viewport.width),
                height: Some(viewport.height),
                ..Style::default()
            },
        );
        // Every visible box, outlined.
        let mut boxes = Vec::new();
        scene.walk(root, |item: &WalkItem| {
            if item.id == root {
                return true;
            }
            let shown = item
                .clip
                .and_then(|clip| clip.intersection(item.bounds.expand(1.)))
                .is_some();
            if shown && boxes.len() < MAX_OUTLINES {
                boxes.push(*item);
            }
            !ui.is_editor(item.id)
        });
        let warned: HashSet<NodeId> = state.warnings.iter().map(|w| w.node).collect();
        while overlay.outlines.len() > boxes.len() {
            let node = overlay.outlines.pop().expect("an outline");
            scene.remove(node);
        }
        while overlay.outlines.len() < boxes.len() {
            let node = scene.append(
                overlay.root,
                NodeKind::Quad(QuadStyle::default()),
                Style {
                    absolute: true,
                    ..Style::default()
                },
            );
            // Before the marks and the panel, which paint over outlines.
            overlay.outlines.push(node);
        }
        for (item, node) in boxes.iter().zip(&overlay.outlines) {
            let warn = warned.contains(&item.id);
            let rect = if warn {
                // A collapsed box still shows where it is.
                Rect::new(
                    item.bounds.x,
                    item.bounds.y,
                    item.bounds.width.max(4.),
                    item.bounds.height.max(4.),
                )
            } else {
                item.bounds
            };
            place(
                &mut scene,
                *node,
                rect,
                QuadStyle {
                    border_color: if warn { WARNED } else { OUTLINE },
                    border_width: if warn { 1.5 } else { 1. },
                    border_over: true,
                    ..QuadStyle::default()
                },
            );
        }
        // Keep the marks and the panel above the outlines.
        let order: Vec<_> = overlay
            .outlines
            .iter()
            .chain(&overlay.marks)
            .chain(std::iter::once(&overlay.panel))
            .copied()
            .collect();
        scene.reorder_children(overlay.root, &order);

        let picked = pointer.and_then(|(x, y)| {
            scene
                .hit_test_all(x, y)
                .into_iter()
                .find(|id| *id != root && !scene.is_within(*id, overlay.root))
        });
        state.picked = picked;
        let labels = self.0.labels.borrow();
        let mut details = vec![Line {
            text: format!(
                "zgx inspector · {} boxes · {} warning{} · {} to close",
                boxes.len(),
                state.warnings.len(),
                if state.warnings.len() == 1 { "" } else { "s" },
                SHORTCUT
            ),
            tone: Tone::Title,
        }];
        let empty = Rect::default();
        match picked {
            Some(id) => {
                let bounds = scene.bounds(id);
                let style = scene.style(id);
                let padding = scene.padding(id);
                let border = border_width(scene.kind(id));
                let margin = margin(&style);
                let outer = Rect::new(
                    bounds.x - margin.left,
                    bounds.y - margin.top,
                    bounds.width + margin.left + margin.right,
                    bounds.height + margin.top + margin.bottom,
                );
                let inner = Rect::new(
                    bounds.x + padding.left + border,
                    bounds.y + padding.top + border,
                    (bounds.width - padding.left - padding.right - 2. * border).max(0.),
                    (bounds.height - padding.top - padding.bottom - 2. * border).max(0.),
                );
                let show_margin = margin != Insets::default();
                let show_content = padding != Insets::default() || border > 0.;
                place(
                    &mut scene,
                    overlay.marks[0],
                    if show_margin { outer } else { empty },
                    QuadStyle {
                        border_color: MARGIN,
                        border_width: 1.,
                        ..QuadStyle::default()
                    },
                );
                place(
                    &mut scene,
                    overlay.marks[1],
                    bounds,
                    QuadStyle {
                        fill: PICKED_FILL,
                        border_color: PICKED_BORDER,
                        border_width: 1.,
                        border_over: true,
                        ..QuadStyle::default()
                    },
                );
                place(
                    &mut scene,
                    overlay.marks[2],
                    if show_content { inner } else { empty },
                    QuadStyle {
                        border_color: CONTENT,
                        border_width: 1.,
                        ..QuadStyle::default()
                    },
                );
                describe(&scene, id, &labels, &state.warnings, &mut details);
            }
            None => {
                for mark in overlay.marks {
                    place(&mut scene, mark, empty, QuadStyle::default());
                }
                details.push(Line {
                    text: "point at a box to inspect it".into(),
                    tone: Tone::Dim,
                });
                for warning in state.warnings.iter().take(4) {
                    details.push(Line {
                        text: format!("⚠ {warning}"),
                        tone: Tone::Warning,
                    });
                }
                if state.warnings.len() > 4 {
                    details.push(Line {
                        text: format!("… {} more on stderr", state.warnings.len() - 4),
                        tone: Tone::Warning,
                    });
                }
            }
        }
        drop(labels);
        if details != state.details {
            for line in overlay.lines.drain(..) {
                scene.remove(line);
            }
            for line in &details {
                let color = match line.tone {
                    Tone::Title => Color(150, 160, 178, 255),
                    Tone::Subject | Tone::Plain => Color(228, 233, 242, 255),
                    Tone::Dim => Color(150, 160, 178, 255),
                    Tone::Source => Color(130, 190, 255, 255),
                    Tone::Warning => Color(255, 175, 90, 255),
                };
                let node = scene.append(
                    overlay.panel,
                    NodeKind::Text {
                        text: line.text.as_str().into(),
                        color,
                        font_size: 11.,
                    },
                    Style {
                        text_wrap: true,
                        ..Style::default()
                    },
                );
                scene.set_font(
                    node,
                    FontStyle {
                        family: FontFamily::Monospace,
                        weight: if line.tone == Tone::Subject { 600 } else { 400 },
                        ..FontStyle::default()
                    },
                );
                overlay.lines.push(node);
            }
            state.details = details;
        }
        // The panel sits in a top corner, away from the pointer and from a
        // script error banner (at the bottom).
        scene.prepare_layout();
        let size = scene.bounds(overlay.panel);
        let margin = 8.;
        let x = match pointer {
            Some((px, _)) if px < viewport.width / 2. => viewport.width - size.width - margin,
            _ => margin,
        };
        let y = margin;
        scene.set_transform(
            overlay.panel,
            Transform {
                x: x.max(0.),
                y: y.max(0.),
            },
        );
        scene.prepare_layout();
    }
}

#[cfg(target_os = "macos")]
const SHORTCUT: &str = "⌃⌥⌘I";
#[cfg(not(target_os = "macos"))]
const SHORTCUT: &str = "Ctrl+Alt+Shift+I";

fn create_overlay(scene: &mut Scene) -> Overlay {
    let root = scene.root();
    let overlay = scene.append(
        root,
        NodeKind::Container(Layout::Overlay),
        Style {
            absolute: true,
            ..Style::default()
        },
    );
    scene.set_inert(overlay, true);
    let mark = |scene: &mut Scene| {
        scene.append(
            overlay,
            NodeKind::Quad(QuadStyle::default()),
            Style {
                absolute: true,
                width: Some(0.),
                height: Some(0.),
                ..Style::default()
            },
        )
    };
    let marks = [mark(scene), mark(scene), mark(scene)];
    let panel = scene.append(
        overlay,
        NodeKind::Panel {
            layout: Layout::Column,
            quad: QuadStyle {
                fill: Color(18, 20, 26, 250),
                radius: 6.,
                border_color: Color(255, 255, 255, 36),
                border_width: 1.,
                ..QuadStyle::default()
            },
        },
        Style {
            absolute: true,
            padding_edges: Some(Insets {
                top: 7.,
                right: 10.,
                bottom: 8.,
                left: 10.,
            }),
            gap: 3.,
            max_width: Some(480.),
            ..Style::default()
        },
    );
    Overlay {
        root: overlay,
        outlines: Vec::new(),
        marks,
        panel,
        lines: Vec::new(),
    }
}

/// Sets an absolutely positioned quad to `rect`.
fn place(scene: &mut Scene, node: NodeId, rect: Rect, quad: QuadStyle) {
    scene.set_style(
        node,
        Style {
            absolute: true,
            width: Some(rect.width.max(0.)),
            height: Some(rect.height.max(0.)),
            ..Style::default()
        },
    );
    scene.set_kind(node, NodeKind::Quad(quad));
    scene.set_transform(
        node,
        Transform {
            x: rect.x,
            y: rect.y,
        },
    );
}

fn border_width(kind: &NodeKind) -> f32 {
    match kind {
        NodeKind::Panel { quad, .. } | NodeKind::Quad(quad) if quad.has_border() => {
            quad.border_width
        }
        _ => 0.,
    }
}
fn margin(style: &Style) -> Insets {
    let mut margin = style.margin;
    if let Some(options) = style.layout_options.as_deref() {
        for (target, value) in [
            (&mut margin.top, options.margin.top),
            (&mut margin.right, options.margin.right),
            (&mut margin.bottom, options.margin.bottom),
            (&mut margin.left, options.margin.left),
        ] {
            if let Some(Length::Px(v)) = value {
                *target = v;
            }
        }
    }
    margin
}

/// A short number: whole pixels without decimals.
fn n(v: f32) -> String {
    if (v - v.round()).abs() < 0.05 {
        format!("{}", v.round() as i64)
    } else {
        format!("{v:.1}")
    }
}
fn insets(i: Insets) -> String {
    if i.top == i.right && i.right == i.bottom && i.bottom == i.left {
        n(i.top)
    } else {
        format!("{} {} {} {}", n(i.top), n(i.right), n(i.bottom), n(i.left))
    }
}
fn length(px: Option<f32>, percent: Option<f32>) -> String {
    match (px, percent) {
        (Some(px), _) => n(px),
        (None, Some(p)) => format!("{}%", n(p * 100.)),
        (None, None) => "auto".into(),
    }
}

/// The node's kind, as the inspector names it.
pub fn kind_name(scene: &Scene, id: NodeId) -> String {
    let layout = |layout: &Layout| match layout {
        Layout::Row => "row",
        Layout::Column => "column",
        Layout::Overlay => "overlay",
    };
    match scene.kind(id) {
        NodeKind::Container(l) => layout(l).into(),
        NodeKind::Panel { layout: l, .. } => format!("{} (panel)", layout(l)),
        NodeKind::Rect(_) => "rect".into(),
        NodeKind::Quad(_) => "quad".into(),
        NodeKind::Image(_) => "image".into(),
        NodeKind::Svg(_) => "svg".into(),
        NodeKind::Canvas(_) => "canvas".into(),
        #[cfg(target_os = "macos")]
        NodeKind::NativeSurface(_) => "native surface".into(),
        NodeKind::RichText { .. } => "rich text".into(),
        NodeKind::Text { text, .. } => {
            let mut shown: String = text.chars().take(32).collect();
            if text.chars().count() > 32 {
                shown.push('…');
            }
            format!("text {shown:?}")
        }
    }
}

/// `id`'s own label, or the nearest labelled ancestor's (`inherited`).
fn nearest_label<'a>(
    scene: &Scene,
    id: NodeId,
    labels: &'a HashMap<NodeId, Label>,
) -> Option<(&'a Label, bool)> {
    if let Some(label) = labels.get(&id) {
        return Some((label, false));
    }
    scene
        .ancestors(id)
        .find_map(|a| labels.get(&a))
        .map(|l| (l, true))
}

fn subject(scene: &Scene, id: NodeId, labels: &HashMap<NodeId, Label>) -> String {
    let kind = kind_name(scene, id);
    match nearest_label(scene, id, labels) {
        // A label names the kind itself when it agrees (`column #list`).
        Some((label, false)) if label.name.split(' ').next() == kind.split(' ').next() => {
            label.name.to_string()
        }
        Some((label, false)) if !label.name.is_empty() => format!("{} · {kind}", label.name),
        Some((label, true)) if !label.name.is_empty() => format!("{kind} in {}", label.name),
        _ => kind,
    }
}

fn describe(
    scene: &Scene,
    id: NodeId,
    labels: &HashMap<NodeId, Label>,
    warnings: &[Warning],
    out: &mut Vec<Line>,
) {
    let plain = |text: String| Line {
        text,
        tone: Tone::Plain,
    };
    let dim = |text: String| Line {
        text,
        tone: Tone::Dim,
    };
    out.push(Line {
        text: subject(scene, id, labels),
        tone: Tone::Subject,
    });
    if let Some((label, inherited)) = nearest_label(scene, id, labels)
        && let Some(source) = &label.source
    {
        out.push(Line {
            text: if inherited {
                format!("created inside {source}")
            } else {
                format!("created at {source}")
            },
            tone: Tone::Source,
        });
    }
    let bounds = scene.bounds(id);
    let style = scene.style(id);
    out.push(plain(format!(
        "{} × {} at ({}, {})",
        n(bounds.width),
        n(bounds.height),
        n(bounds.x),
        n(bounds.y)
    )));
    let mut sizes = format!(
        "w {} · h {}",
        length(style.width, style.width_percent),
        length(style.height, style.height_percent)
    );
    for (name, value) in [
        ("min w", style.min_width),
        ("max w", style.max_width),
        ("min h", style.min_height),
        ("max h", style.max_height),
    ] {
        if let Some(v) = value {
            sizes.push_str(&format!(" · {name} {}", n(v)));
        }
    }
    if style.absolute {
        sizes.push_str(" · absolute");
    }
    out.push(dim(sizes));
    let padding = scene.padding(id);
    let margin = margin(&style);
    let border = border_width(scene.kind(id));
    let mut spacing = Vec::new();
    if padding != Insets::default() {
        spacing.push(format!("padding {}", insets(padding)));
    }
    if margin != Insets::default() {
        spacing.push(format!("margin {}", insets(margin)));
    }
    if border > 0. {
        spacing.push(format!("border {}", n(border)));
    }
    if style.gap > 0. {
        spacing.push(format!("gap {}", n(style.gap)));
    }
    if !spacing.is_empty() {
        out.push(dim(spacing.join(" · ")));
    }
    let container = matches!(
        scene.kind(id),
        NodeKind::Container(_) | NodeKind::Panel { .. }
    );
    let mut flex = format!(
        "grow {} · shrink {}",
        n(style.flex_grow),
        n(style.flex_shrink)
    );
    if container {
        flex.push_str(&format!(
            " · align {:?} · justify {:?}",
            style.align, style.justify
        ));
    }
    if let Some(options) = style.layout_options.as_deref() {
        if let Some(Length::Px(b)) = options.basis {
            flex.push_str(&format!(" · basis {}", n(b)));
        }
        if let Some(Length::Percent(b)) = options.basis {
            flex.push_str(&format!(" · basis {}%", n(b * 100.)));
        }
        if let Some(wrap) = options.wrap {
            flex.push_str(&format!(" · wrap {wrap:?}"));
        }
    }
    out.push(dim(flex.to_lowercase()));
    let (cx, cy) = scene.clips(id);
    let scrolls = scene
        .children(id)
        .iter()
        .any(|c| scene.is_scroll_content(*c));
    let own = match (cx, cy) {
        (true, true) => Some("clips its content"),
        (true, false) => Some("clips its content horizontally"),
        (false, true) => Some("clips its content vertically"),
        (false, false) => None,
    };
    if let Some(own) = own {
        out.push(dim(if scrolls {
            format!("{own} (scrolls)")
        } else {
            own.to_string()
        }));
    }
    if let Some(visible) = scene.visible_bounds(id)
        && visible != bounds
    {
        out.push(dim(format!(
            "clipped to {} × {} at ({}, {})",
            n(visible.width),
            n(visible.height),
            n(visible.x),
            n(visible.y)
        )));
    } else if scene.visible_bounds(id).is_none() {
        out.push(dim("not visible (clipped or transparent)".into()));
    }
    for warning in warnings.iter().filter(|w| w.node == id) {
        out.push(Line {
            text: format!("⚠ {}", warning.message),
            tone: Tone::Warning,
        });
    }
}

/// Whether `id` shows anything: text, a box with an area, or such a
/// descendant in flow.
fn shows_content(scene: &Scene, id: NodeId, depth: usize) -> bool {
    let b = scene.bounds(id);
    if b.width > 0.5 && b.height > 0.5 {
        return true;
    }
    let text = match scene.kind(id) {
        NodeKind::Text { text, .. } => !text.trim().is_empty(),
        NodeKind::RichText { text } => !text.text().trim().is_empty(),
        _ => false,
    };
    text || (depth < 32
        && scene.children(id).iter().any(|c| {
            scene.layout_visible(*c)
                && !scene.style(*c).absolute
                && shows_content(scene, *c, depth + 1)
        }))
}

/// The layout warnings of `ui`'s current layout, unlabelled. Call after
/// layout (`Ui::prepare_frame`).
pub fn layout_warnings(ui: &Ui) -> Vec<Warning> {
    check(ui, &HashMap::new())
}

fn check(ui: &Ui, labels: &HashMap<NodeId, Label>) -> Vec<Warning> {
    let scene = ui.scene.borrow();
    let root = scene.root();
    let mut warnings = Vec::new();
    // Zero-size boxes already reported: their collapsed descendants are not.
    let mut collapsed = HashSet::new();
    scene.walk(root, |item| {
        let id = item.id;
        if id == root {
            return true;
        }
        let style = scene.style(id);
        let warn = |kind, message: String| Warning {
            node: id,
            kind,
            subject: subject(&scene, id, labels),
            source: nearest_label(&scene, id, labels).and_then(|(l, _)| l.source.clone()),
            message,
        };
        // A zero-size box with content.
        let in_flow = |c: &&NodeId| scene.layout_visible(**c) && !scene.style(**c).absolute;
        let children = scene.children(id).iter().filter(in_flow).count();
        let text = match scene.kind(id) {
            NodeKind::Text { text, .. } => !text.trim().is_empty(),
            NodeKind::RichText { text } => !text.text().trim().is_empty(),
            _ => false,
        };
        // Deliberately collapsed (`w = 0`, `max_h = 0`): not suspicious.
        let deliberate_w = style.width == Some(0.) || style.max_width == Some(0.);
        let deliberate_h = style.height == Some(0.) || style.max_height == Some(0.);
        let zero_w = item.bounds.width <= 0.5 && !deliberate_w;
        let zero_h = item.bounds.height <= 0.5 && !deliberate_h;
        let parent_collapsed = scene.parent(id).is_some_and(|p| collapsed.contains(&p));
        // Children that show something: an empty placeholder (a hidden
        // `ui.show`'s region) is no content.
        let showing = if zero_w || zero_h {
            scene
                .children(id)
                .iter()
                .filter(in_flow)
                .filter(|c| shows_content(&scene, **c, 0))
                .count()
        } else {
            0
        };
        if (zero_w || zero_h) && (showing > 0 || text) && !parent_collapsed {
            let axis = match (zero_w, zero_h) {
                (true, true) => "zero size",
                (true, false) => "zero width",
                _ => "zero height",
            };
            let content = if text {
                "text".to_string()
            } else if showing == 1 {
                "a child".to_string()
            } else {
                format!("{showing} children")
            };
            let hint = if zero_h && !zero_w {
                "give it a height, or let it grow (grow = 1) in a parent that has one"
            } else {
                "give it a width, or let it grow (grow = 1) in a parent that has one"
            };
            warnings.push(warn(
                WarningKind::ZeroSize,
                format!(
                    "{axis} ({} × {}) with {content}; {hint}",
                    n(item.bounds.width),
                    n(item.bounds.height)
                ),
            ));
            collapsed.insert(id);
        } else if parent_collapsed && (zero_w || zero_h) {
            collapsed.insert(id);
        }
        // Content overflowing a clip that does not scroll.
        let (clip_x, clip_y) = scene.clips(id);
        let scrolls = scene
            .children(id)
            .iter()
            .any(|c| scene.is_scroll_content(*c));
        if (clip_x || clip_y) && !scrolls && !ui.is_editor(id) && children > 0 {
            let mut extent: Option<Rect> = None;
            for child in scene.children(id).iter().filter(in_flow) {
                // As laid out: a translation (an animation, a fill that
                // slides in) is deliberate.
                let mut b = scene.bounds(*child);
                let t = scene.transform(*child);
                b.x -= t.x;
                b.y -= t.y;
                extent = Some(extent.map_or(b, |e| e.union(b)));
            }
            let b = item.bounds;
            let t = scene.transform(id);
            let b = Rect::new(b.x - t.x, b.y - t.y, b.width, b.height);
            if let Some(e) = extent {
                let over_x = (b.x - e.x).max(e.x + e.width - (b.x + b.width));
                let over_y = (b.y - e.y).max(e.y + e.height - (b.y + b.height));
                let axis = if clip_y && over_y > 0.5 {
                    Some(("vertically", over_y, e.height, b.height))
                } else if clip_x && over_x > 0.5 {
                    Some(("horizontally", over_x, e.width, b.width))
                } else {
                    None
                };
                if let Some((axis, over, content, size)) = axis {
                    warnings.push(warn(
                        WarningKind::Overflow,
                        format!(
                            "content overflows its clip {axis} by {} ({} in {}) and it does not scroll; use ui.scroll, or make room",
                            n(over),
                            n(content),
                            n(size)
                        ),
                    ));
                }
            }
        }
        // A percentage against a parent that sizes to its content.
        let (pw, ph) = scene.unresolved_percentages(id);
        if pw || ph {
            let (axis, value, size) = if pw {
                ("width", style.width_percent, item.bounds.width)
            } else {
                ("height", style.height_percent, item.bounds.height)
            };
            warnings.push(warn(
                WarningKind::UnresolvedPercent,
                format!(
                    "{axis} {}% has nothing to resolve against: its parent's {axis} depends on its content, so it falls back to its own ({}); give the parent a {axis}",
                    n(value.unwrap_or(0.) * 100.),
                    n(size)
                ),
            ));
        }
        // Editors' insides are theirs.
        !ui.is_editor(id)
    });
    warnings
}
