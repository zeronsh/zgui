//! Selecting display text with the pointer, as on a web page. A selectable
//! region (`View::selectable`, CSS `user-select: text`) holds one selection
//! at a time: from an anchor to a focus, across any number of its text
//! nodes in document order. Dragging selects; a double press selects a
//! word and a triple press a paragraph, and dragging after either extends
//! by whole words or paragraphs; Shift extends; select-all and copy work
//! with the keyboard wherever the focus is not in an editor.
//!
//! Nothing here runs for a document without selectable regions. With one,
//! a press shapes the text it lands on (once: layouts are kept while the
//! selection is made), and a change of selection repaints only the text
//! whose highlight changed (`Scene::set_text_highlight`).
use crate::{
    input::{EventPhase, InputEvent, Key, Modifiers, PointerButton},
    scene::{Color, NodeId, NodeKind, Rect, Scene, TextHighlight, TextLeaf},
    semantics::Role,
    text_layout::{FontStyle, TextLayout},
    theme::tok,
    widgets::{Ui, WeakUi},
};
use std::{
    cmp::Ordering,
    collections::HashMap,
    ops::Range,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

/// A place in selectable text: a text node and a UTF-8 byte offset into
/// its text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextPoint {
    /// The text node.
    pub node: NodeId,
    /// The UTF-8 byte offset into its text.
    pub offset: usize,
}

/// Where the pointer is in selectable text, and what is there.
struct Hit {
    point: TextPoint,
    position: crate::text_layout::TextPosition,
    /// The pointer in the node's box.
    local: (f32, f32),
    text: Arc<str>,
    layout: Rc<dyn TextLayout>,
}
impl Hit {
    /// The word or paragraph at the pointer (the point itself for a
    /// character).
    fn edges(&self, unit: Unit) -> (TextPoint, TextPoint) {
        let at = self.point.offset;
        let (start, end) = match unit {
            Unit::Character => (at, at),
            Unit::Word => {
                let (start, end) = crate::widgets::editor_pointer_unit(
                    (0, std::borrow::Cow::Borrowed(&*self.text)),
                    self.layout.as_ref(),
                    self.position,
                    self.local.0,
                    self.local.1,
                    false,
                    true,
                );
                (start.byte_offset, end.byte_offset)
            }
            Unit::Paragraph => (
                self.text[..at].rfind('\n').map_or(0, |i| i + 1),
                self.text[at..]
                    .find('\n')
                    .map_or(self.text.len(), |i| at + i),
            ),
        };
        let node = self.point.node;
        (
            TextPoint {
                node,
                offset: start,
            },
            TextPoint { node, offset: end },
        )
    }
}

/// What a drag extends by: the unit its press selected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Unit {
    Character,
    Word,
    Paragraph,
}

struct Gesture {
    root: NodeId,
    unit: Unit,
    /// The word or paragraph first pressed: a drag keeps it selected,
    /// extending from its far edge.
    edges: (TextPoint, TextPoint),
    /// The pointer moved since the press: a release selects, not clicks.
    dragged: bool,
    /// Where the pointer is: held past a scroll viewport's edge, the
    /// viewport scrolls toward it.
    pointer: (f32, f32),
}

/// Where a point in a virtual list's row lies in the list: the row's index
/// and the path of child positions from the row to its text node, so a
/// selection outlives the row's nodes as the list recycles them.
#[derive(Clone, Debug, PartialEq)]
struct RowAddress {
    content: NodeId,
    index: usize,
    path: Vec<u32>,
    /// The row's text as a copy takes it, when the end was set there, and
    /// where the end lies in it: a copy of an end whose row has scrolled
    /// away cuts the row where the selection did.
    copied: Option<(Rc<str>, usize)>,
}

/// A selection's end, as it stands: in mounted text, in a row the list has
/// unmounted (scrolled away; it comes back with its row), or gone.
enum End {
    Live(TextPoint),
    Parked(RowAddress, usize),
    Gone,
}

/// A press on a link or button in selectable text: a drag from it selects.
#[derive(Clone, Copy)]
struct Pending {
    root: NodeId,
    x: f32,
    y: f32,
}

/// What a text node was shaped from, telling when its layout is stale.
#[derive(PartialEq)]
enum Source {
    Plain {
        text: Arc<str>,
        size: u32,
        font: FontStyle,
    },
    Rich(Arc<crate::rich_text::RichText>),
}
impl Source {
    fn of(scene: &Scene, id: NodeId) -> Option<Self> {
        match scene.kind(id) {
            NodeKind::Text {
                text, font_size, ..
            } => Some(Self::Plain {
                text: text.clone(),
                size: font_size.to_bits(),
                font: scene.font(id).clone(),
            }),
            NodeKind::RichText { text } => Some(Self::Rich(text.clone())),
            _ => None,
        }
    }
}

struct Shaped {
    node: NodeId,
    source: Source,
    width: u32,
    revision: u64,
    layout: Rc<dyn TextLayout>,
}
/// Layouts kept for the nodes a gesture hits: the few it moves over.
const KEPT_LAYOUTS: usize = 8;

/// A node showing part of the selection, and what its highlight was laid
/// out from.
#[derive(Clone, PartialEq)]
struct Shown {
    node: NodeId,
    range: Range<usize>,
    /// Its box's size and its text (pointer and length), and the shaper.
    stamp: (u32, u32, usize, usize, u64),
}

#[derive(Default)]
pub(crate) struct State {
    /// Mounted selectable regions and their highlight colours (in a
    /// focused window, and in one at the back).
    roots: HashMap<NodeId, Palette>,
    root: Option<NodeId>,
    anchor: Option<TextPoint>,
    focus: Option<TextPoint>,
    shown: Vec<Shown>,
    layouts: Vec<Shaped>,
    gesture: Option<Gesture>,
    pending: Option<Pending>,
    /// A long press selected a word over a control (a link, a button) that
    /// held the press: the region takes the pointer from it at the next
    /// move, so it hears a cancel and never activates.
    take_from: Option<NodeId>,
    /// The region last pressed in: what select-all selects.
    active_root: Option<NodeId>,
    /// The window is in the background: highlights take the inactive colour.
    inactive: bool,
    /// Bumped by every change of selection.
    revision: u64,
    /// A press in this dispatch went to a selectable region.
    pressed: bool,
    /// The scene's revisions the highlights were laid out at.
    synced: (u64, u64, u64),
    /// The region's text, gathered for each refresh (kept for its room).
    leaves: Vec<TextLeaf>,
    /// Virtual list rows in selectable regions, and their indices.
    rows: HashMap<NodeId, crate::reactive::Signal<usize>>,
    /// Virtual lists' row text for copying rows they have not mounted
    /// (`View::row_text`), by the node holding their rows.
    row_texts: HashMap<NodeId, RowText>,
    /// Where the anchor and the focus lie in virtual lists, if they do.
    anchor_row: Option<RowAddress>,
    focus_row: Option<RowAddress>,
    /// A drag's autoscroll: its next tick, and when the last ran.
    scroll: Option<(Instant, Instant)>,
    /// The region whose semantic node holds the selection.
    semantic_root: Option<NodeId>,
    /// Rich text paragraphs' links with somewhere to go: their ranges and
    /// targets, for copied HTML.
    links: HashMap<NodeId, Links>,
    /// Select-all chose the whole region: its ends follow its first and
    /// last text as virtual lists mount rows, and a copy takes every row.
    all: bool,
    /// Selected text out of view, its highlight not laid out yet: each is
    /// when it first shows (a select-all over a long document lays out a
    /// screenful, not every paragraph), checked as the geometry changes.
    deferred: Vec<Shown>,
    /// The scene's geometry when `deferred` was last checked.
    deferred_at: u64,
}

/// Rows a copy takes from one list's row text at most.
const MAX_PROVIDED_ROWS: usize = 1_000_000;

/// A virtual list's plain text of row `index`, `None` past its end.
pub(crate) type RowText = Rc<dyn Fn(usize) -> Option<String>>;

/// Removes a virtual list's row text once it is unmounted.
struct RowTextGuard {
    ui: WeakUi,
    content: NodeId,
}
impl Drop for RowTextGuard {
    fn drop(&mut self) {
        if let Some(ui) = self.ui.upgrade()
            && let Ok(mut state) = ui.selection_state().try_borrow_mut()
        {
            state.row_texts.remove(&self.content);
        }
    }
}

/// The virtual list at `root` copies the rows a selection spans and it has
/// not mounted from `text` (`View::row_text`).
pub(crate) fn mount_row_text(ui: &mut Ui, root: NodeId, text: RowText) {
    // Only a selection copies it.
    if !ui.scene.borrow().is_selectable(root) {
        return;
    }
    // Its rows' parent: the list's content, in its viewport.
    let content = {
        let scene = ui.scene.borrow();
        let viewport = scene.children(root).first().copied();
        viewport.and_then(|viewport| scene.children(viewport).first().copied())
    };
    let Some(content) = content else {
        return;
    };
    ui.selection_state()
        .borrow_mut()
        .row_texts
        .insert(content, text);
    let guard = RowTextGuard {
        ui: ui.downgrade(),
        content,
    };
    ui.retain(root, guard);
}

/// A paragraph's links: each one's range in its text and where it goes.
pub(crate) type Links = Vec<(Range<usize>, Arc<str>)>;
/// Rich text `paragraph`'s links (their ranges and targets), as copied
/// HTML writes them.
pub(crate) fn set_links(ui: &Ui, paragraph: NodeId, links: Links) {
    let mut state = ui.selection_state().borrow_mut();
    if links.is_empty() {
        state.links.remove(&paragraph);
    } else {
        state.links.insert(paragraph, links);
    }
}
/// Forgets a paragraph's links once it is unmounted.
pub(crate) struct LinksGuard {
    pub(crate) ui: WeakUi,
    pub(crate) paragraph: NodeId,
}
impl Drop for LinksGuard {
    fn drop(&mut self) {
        if let Some(ui) = self.ui.upgrade()
            && let Ok(mut state) = ui.selection_state().try_borrow_mut()
        {
            state.links.remove(&self.paragraph);
        }
    }
}

/// Removes a virtual row's entry once it is unmounted.
struct RowGuard {
    ui: WeakUi,
    row: NodeId,
}
impl Drop for RowGuard {
    fn drop(&mut self) {
        if let Some(ui) = self.ui.upgrade()
            && let Ok(mut state) = ui.selection_state().try_borrow_mut()
        {
            state.rows.remove(&self.row);
        }
    }
}

/// A virtual list mounted `row` at `index`: a selection in a selectable
/// region outlives the row's nodes, coming back when the row does.
pub(crate) fn mount_row(ui: &mut Ui, row: NodeId, index: &crate::reactive::Signal<usize>) {
    if !ui.scene.borrow().is_selectable(row) {
        return;
    }
    ui.selection_state()
        .borrow_mut()
        .rows
        .insert(row, index.clone());
    let guard = RowGuard {
        ui: ui.downgrade(),
        row,
    };
    ui.retain(row, guard);
}

/// Removes a region's entry once it is unmounted.
struct RootGuard {
    ui: WeakUi,
    root: NodeId,
}
impl Drop for RootGuard {
    fn drop(&mut self) {
        if let Some(ui) = self.ui.upgrade()
            && let Ok(mut state) = ui.selection_state().try_borrow_mut()
        {
            state.roots.remove(&self.root);
            if state.active_root == Some(self.root) {
                state.active_root = None;
            }
            // Unmounted under a drag: the drag is over.
            if state.gesture.as_ref().is_some_and(|g| g.root == self.root) {
                state.gesture = None;
                state.scroll = None;
                state.pending = None;
            }
        }
    }
}

/// A region's highlight, in a focused window (`[0]`) and one at the back
/// (`[1]`): its background, the selected glyphs' colour (none keeps their
/// own) and the corner radius.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Palette {
    background: [Color; 2],
    text: [Option<Color>; 2],
    radius: f32,
}
impl Palette {
    fn highlight(&self, inactive: bool, rects: Vec<Rect>) -> TextHighlight {
        let at = usize::from(inactive);
        TextHighlight {
            color: self.background[at],
            rects,
            text_color: self.text[at],
            radius: self.radius,
        }
    }
}

/// A region's own highlight colours (`View::selection_color`,
/// `selected_text_color`), over the theme's, in either window state.
#[derive(Default)]
pub(crate) struct SelectionStyle {
    pub(crate) background: Option<Box<dyn FnMut() -> Color>>,
    pub(crate) text: Option<Box<dyn FnMut() -> Color>>,
}

/// Make `root` a selectable region (`View::selectable(true)`).
pub(crate) fn mount_root(ui: &mut Ui, root: NodeId, mut style: SelectionStyle) {
    ui.scene.borrow_mut().set_user_select(root, Some(true));
    // A document, as a browser's page is: assistive technology reads its
    // text and finds its selection on it (platform text APIs, UIA's
    // TextPattern and NSAccessibility's text ranges, serve documents, not
    // groups). A region with a role of its own (a window, a scroll view)
    // keeps it.
    let role = ui.semantics.borrow().get(root).map(|node| node.role);
    match role {
        None => ui.semantics.borrow_mut().set(
            root,
            crate::semantics::SemanticNode::new(Role::Document, ""),
        ),
        Some(Role::Group) => ui
            .semantics
            .borrow_mut()
            .update(root, |node| node.role = Role::Document),
        Some(_) => {}
    }
    let theme = ui.theme_scope();
    let weak = ui.downgrade();
    ui.bind(root, move || {
        // Transparent selected-text colours keep the glyphs' own.
        let text = |color: Color| (color.3 > 0).then_some(color);
        let mut colors = Palette {
            background: [
                theme.color(tok::SELECTION),
                theme.color(tok::SELECTION_INACTIVE),
            ],
            text: [
                text(theme.color(tok::SELECTION_TEXT)),
                text(theme.color(tok::SELECTION_TEXT_INACTIVE)),
            ],
            radius: theme.number(tok::SELECTION_RADIUS).max(0.),
        };
        if let Some(background) = &mut style.background {
            colors.background = [background(); 2];
        }
        if let Some(color) = &mut style.text {
            colors.text = [text(color()); 2];
        }
        let Some(ui) = weak.upgrade() else {
            return;
        };
        let recolor = {
            let mut state = ui.selection_state().borrow_mut();
            let old = state.roots.insert(root, colors);
            old.is_some_and(|old| old != colors) && state.root == Some(root)
        };
        if recolor {
            ui.refresh_selection(true);
        }
    });
    let guard = RootGuard {
        ui: ui.downgrade(),
        root,
    };
    ui.retain(root, guard);
    let weak = ui.downgrade();
    ui.on_event(root, false, move |cx| {
        if cx.phase == EventPhase::Capture {
            return;
        }
        let Some(ui) = weak.upgrade() else {
            return;
        };
        // An inner region inside a selectable one is part of the outer.
        if ui.scene.borrow().selection_root(root) != Some(root) {
            return;
        }
        match cx.event {
            InputEvent::PointerDown {
                x,
                y,
                button: PointerButton::Primary,
            } => {
                // A control-click on macOS is a secondary click: it opens the
                // context menu on the selection rather than collapsing it.
                if cx.default_prevented()
                    || context_click(cx.pointer_modifiers())
                    || !ui.selectable_target(root, cx.target)
                {
                    return;
                }
                ui.selection_state().borrow_mut().pressed = true;
                // A control under the press keeps it (a slider drags, a
                // button presses); a link or button gives it up to a drag.
                if cx.capture_requested() && cx.target != root {
                    if ui.yields_drag(root, cx.target) {
                        ui.selection_state().borrow_mut().pending = Some(Pending { root, x, y });
                    }
                    return;
                }
                let modifiers = cx.pointer_modifiers();
                if ui.press_selection(root, x, y, cx.click_count().max(1), modifiers) {
                    cx.capture_pointer();
                }
            }
            InputEvent::PointerMove { x, y } => {
                let take = ui
                    .selection_state()
                    .borrow_mut()
                    .take_from
                    .take_if(|from| *from == root);
                if take.is_some() {
                    cx.take_pointer();
                }
                let pending = ui.selection_state().borrow().pending;
                if let Some(pending) = pending.filter(|p| p.root == root) {
                    let (dx, dy) = (x - pending.x, y - pending.y);
                    if dx.hypot(dy) <= cx.drag_threshold() {
                        return;
                    }
                    ui.selection_state().borrow_mut().pending = None;
                    if ui.press_selection(root, pending.x, pending.y, 1, Modifiers::default()) {
                        cx.take_pointer();
                    } else {
                        return;
                    }
                }
                if ui.drag_selection(root, x, y) {
                    cx.prevent_default();
                }
            }
            InputEvent::PointerUp {
                button: PointerButton::Primary,
                ..
            } => {
                ui.selection_state().borrow_mut().pending = None;
                // A press that selected (dragged) does not also click.
                if ui.end_selection_gesture(root) {
                    cx.prevent_default();
                }
            }
            // Cancelled under it (another press took the pointer), or here.
            InputEvent::PointerCancel if cx.target == root => {
                ui.selection_state().borrow_mut().pending = None;
                ui.end_selection_gesture(root);
            }
            _ => {}
        }
    });
}

impl Ui {
    /// Whether a press on `target` in `root` can begin a selection: it is
    /// in selectable text, not in an editor (which selects its own).
    fn selectable_target(&self, root: NodeId, target: NodeId) -> bool {
        let scene = self.scene.borrow();
        if target != root && scene.selection_root(target) != Some(root) {
            return false;
        }
        !std::iter::once(target)
            .chain(scene.ancestors(target))
            .take_while(|id| *id != root)
            .any(|id| self.is_editor(id))
    }
    /// Whether the control under a press in `root` gives the pointer up to
    /// a drag: a link or a button, not a slider or a scrollbar.
    fn yields_drag(&self, root: NodeId, target: NodeId) -> bool {
        let scene = self.scene.borrow();
        let semantics = self.semantics.borrow();
        std::iter::once(target)
            .chain(scene.ancestors(target))
            .take_while(|id| *id != root)
            .find_map(|id| semantics.get(id).map(|node| node.role))
            .is_some_and(|role| matches!(role, Role::Link | Role::Button))
    }
    /// A primary press at `x`, `y` in `root`: begin a selection. Returns
    /// whether a gesture began (the pointer is wanted).
    fn press_selection(
        &self,
        root: NodeId,
        x: f32,
        y: f32,
        count: u8,
        modifiers: Modifiers,
    ) -> bool {
        let point = self.point_at(root, x, y);
        let mut state = self.selection_state().borrow_mut();
        state.active_root = Some(root);
        let Some(hit) = point else {
            drop(state);
            self.set_selection(None, None, None);
            return false;
        };
        let extend = modifiers.shift && state.root == Some(root) && state.anchor.is_some();
        let unit = match count {
            _ if extend => Unit::Character,
            2 => Unit::Word,
            3 => Unit::Paragraph,
            _ => Unit::Character,
        };
        let edges = hit.edges(unit);
        let anchor = if extend {
            state.anchor.unwrap()
        } else {
            edges.0
        };
        state.gesture = Some(Gesture {
            root,
            unit,
            edges: if extend { (anchor, anchor) } else { edges },
            dragged: false,
            pointer: (x, y),
        });
        drop(state);
        self.set_selection(Some(root), Some(anchor), Some(edges.1));
        true
    }
    /// The pointer moved to `x`, `y` during a gesture in `root`.
    fn drag_selection(&self, root: NodeId, x: f32, y: f32) -> bool {
        let (unit, edges) = {
            let mut state = self.selection_state().borrow_mut();
            let Some(gesture) = state.gesture.as_mut().filter(|g| g.root == root) else {
                return false;
            };
            gesture.dragged = true;
            gesture.pointer = (x, y);
            (gesture.unit, gesture.edges)
        };
        let Some(hit) = self.point_at(root, x, y) else {
            return true;
        };
        // The word or paragraph pressed was recycled with its row: the
        // drag goes on from the anchor.
        let recycled = {
            let scene = self.scene.borrow();
            !scene.contains(edges.0.node) || !scene.contains(edges.1.node)
        };
        let (anchor, focus) = if unit == Unit::Character || recycled {
            let anchor = match unit {
                Unit::Character => edges.0,
                _ => self.selection_state().borrow().anchor.unwrap_or(edges.0),
            };
            (anchor, hit.point)
        } else {
            let scene = self.scene.borrow();
            let current = hit.edges(unit);
            if order(&scene, current.0, edges.0).is_lt() {
                (edges.1, current.0)
            } else if order(&scene, current.1, edges.1).is_gt() {
                (edges.0, current.1)
            } else {
                edges
            }
        };
        self.set_selection(Some(root), Some(anchor), Some(focus));
        self.arm_selection_scroll();
        true
    }
    /// The press that began a gesture in `root` ended. Returns whether the
    /// gesture selected by dragging.
    fn end_selection_gesture(&self, root: NodeId) -> bool {
        let mut state = self.selection_state().borrow_mut();
        let Some(gesture) = state.gesture.take_if(|g| g.root == root) else {
            return false;
        };
        state.layouts.clear();
        state.scroll = None;
        gesture.dragged
            && state
                .anchor
                .zip(state.focus)
                .is_some_and(|(anchor, focus)| anchor != focus)
    }
    /// The selectable text point nearest `x`, `y` in `root`, and a way to
    /// find the word or paragraph there. Text the pointer is above or
    /// below is entered at its start or end; text clipped by a scroll
    /// viewport is hit at its visible edge.
    fn point_at(&self, root: NodeId, x: f32, y: f32) -> Option<Hit> {
        self.point_near(root, x, y, true)
    }
    /// `point_at`; not `visible`, over all the region's laid-out text, at a
    /// point already in its layout space: the keyboard's steps, which reach
    /// text scrolled out of view.
    fn point_near(&self, root: NodeId, x: f32, y: f32, visible: bool) -> Option<Hit> {
        let only_visible = visible;
        let scene = self.scene.borrow();
        // The nearest text, its distance and the pointer in its space.
        type Nearest = (TextLeaf, (f32, f32), (f32, f32));
        let mut best: Option<Nearest> = None;
        scene.selectable_text(
            root,
            |id| self.is_editor(id),
            |leaf| {
                let Some(visible) = (if only_visible {
                    leaf.visible
                } else {
                    Some(leaf.bounds)
                }) else {
                    return;
                };
                // The pointer where the text is laid out, through scales
                // and rotations on the way.
                let Some((x, y)) = (match only_visible {
                    true => scene.point_in(leaf.id, x, y),
                    false => Some((x, y)),
                }) else {
                    return;
                };
                let distance = |at: f32, start: f32, extent: f32| {
                    if at < start {
                        start - at
                    } else if at >= start + extent {
                        at - (start + extent) + 0.01
                    } else {
                        0.
                    }
                };
                let key = (
                    distance(y, visible.y, visible.height),
                    distance(x, visible.x, visible.width),
                );
                // Ties go to what paints later, over the rest.
                if best.is_none_or(|(_, best, _)| key <= best) {
                    best = Some((leaf, key, (x, y)));
                }
            },
        );
        let (leaf, _, (x, y)) = best?;
        let bounds = leaf.bounds;
        let visible = match only_visible {
            true => leaf.visible.unwrap(),
            false => bounds,
        };
        let layout = self.selection_layout(&scene, leaf.id)?;
        let text: Arc<str> = text_of(&scene, leaf.id)?;
        let end = text.len();
        // Past an edge of the text (not of its clip): its start or end.
        let offset = if y < visible.y && bounds.y >= visible.y - 0.5 {
            Some(0)
        } else if y >= visible.y + visible.height
            && bounds.y + bounds.height <= visible.y + visible.height + 0.5
        {
            Some(end)
        } else {
            None
        };
        let local = (
            x.clamp(visible.x, visible.x + visible.width) - bounds.x,
            y.clamp(
                visible.y,
                (visible.y + visible.height - 0.01).max(visible.y),
            ) - bounds.y,
        );
        let hit = match offset {
            Some(offset) => crate::text_layout::TextPosition {
                byte_offset: offset,
                ..Default::default()
            },
            None => layout.hit_position(local.0, local.1),
        };
        Some(Hit {
            point: TextPoint {
                node: leaf.id,
                offset: hit.byte_offset.min(end),
            },
            position: hit,
            local,
            text,
            layout,
        })
    }
    /// `id`'s layout as it paints, kept while a gesture moves over it.
    fn selection_layout(&self, scene: &Scene, id: NodeId) -> Option<Rc<dyn TextLayout>> {
        let source = Source::of(scene, id)?;
        let width = scene.layout_bounds(id).width;
        let revision = scene.text_shaper_revision();
        let mut state = self.selection_state().borrow_mut();
        if let Some(index) = state.layouts.iter().position(|s| s.node == id) {
            let kept = &state.layouts[index];
            if kept.width == width.to_bits() && kept.revision == revision && kept.source == source {
                return Some(kept.layout.clone());
            }
            state.layouts.remove(index);
        }
        drop(state);
        let layout = shape(scene, id, width)?;
        let mut state = self.selection_state().borrow_mut();
        if state.layouts.len() >= KEPT_LAYOUTS {
            state.layouts.remove(0);
        }
        state.layouts.push(Shaped {
            node: id,
            source,
            width: width.to_bits(),
            revision,
            layout: layout.clone(),
        });
        Some(layout)
    }
    /// Replace the selection and show it.
    fn set_selection(
        &self,
        root: Option<NodeId>,
        anchor: Option<TextPoint>,
        focus: Option<TextPoint>,
    ) {
        {
            let mut state = self.selection_state().borrow_mut();
            if state.root == root && state.anchor == anchor && state.focus == focus {
                return;
            }
            // An end kept as it was keeps its row; a new one in a virtual
            // list's row takes that row's address.
            let rows = {
                let scene = self.scene.borrow();
                let row = |point: Option<TextPoint>,
                           old: Option<TextPoint>,
                           kept: &Option<RowAddress>| match point {
                    Some(point) if Some(point) == old => kept.clone(),
                    Some(point) => row_address(&state.rows, &scene, point),
                    None => None,
                };
                (
                    row(anchor, state.anchor, &state.anchor_row),
                    row(focus, state.focus, &state.focus_row),
                )
            };
            (state.anchor_row, state.focus_row) = rows;
            state.root = root;
            state.anchor = anchor;
            state.focus = focus;
            state.all = false;
            state.revision = state.revision.wrapping_add(1);
        }
        self.refresh_selection(false);
    }
    /// Lay the selection's highlights out again: only the text whose part of
    /// it, or whose layout, changed (all of it if `recolor`).
    pub(crate) fn refresh_selection(&self, recolor: bool) {
        // All of a region: from its first text to its last, as they are now.
        let all = {
            let state = self.selection_state().borrow();
            state.root.filter(|_| state.all)
        };
        if let Some(root) = all.filter(|root| self.scene.borrow().contains(*root)) {
            let (first, last) = self.region_ends(root);
            let mut state = self.selection_state().borrow_mut();
            if first.is_some() {
                (state.anchor, state.focus) = (first, last);
                (state.anchor_row, state.focus_row) = (None, None);
            }
        }
        let mut state = self.selection_state().borrow_mut();
        // Ends in recycled virtual rows: back in their rows if mounted again.
        let (anchor, focus) = {
            let scene = self.scene.borrow();
            let end = |point: Option<TextPoint>, row: &Option<RowAddress>| match point {
                Some(point) => revive(&state.rows, &scene, point, row.as_ref()),
                None => End::Gone,
            };
            (
                end(state.anchor, &state.anchor_row),
                end(state.focus, &state.focus_row),
            )
        };
        if let End::Live(point) = anchor {
            state.anchor = Some(point);
        }
        if let End::Live(point) = focus {
            state.focus = Some(point);
        }
        let root = state
            .root
            .filter(|root| self.scene.borrow().contains(*root));
        let selected = root.filter(|_| !matches!(anchor, End::Gone) && !matches!(focus, End::Gone));
        // Its text is gone: so is the selection.
        if selected.is_none() && state.anchor.is_some() {
            state.root = None;
            state.anchor = None;
            state.focus = None;
            state.anchor_row = None;
            state.focus_row = None;
            state.revision = state.revision.wrapping_add(1);
        }
        let mut shown = Vec::new();
        let mut backwards = false;
        let mut leaves = std::mem::take(&mut state.leaves);
        let colors = selected.and_then(|root| state.roots.get(&root).copied());
        if let (Some(root), Some(_)) = (selected, colors) {
            let scene = self.scene.borrow();
            let rows = &state.rows;
            backwards = order_ends(rows, &scene, &anchor, &focus).is_gt();
            let (start, end) = if backwards {
                (focus, anchor)
            } else {
                (anchor, focus)
            };
            leaves.clear();
            scene.selectable_text(root, |id| self.is_editor(id), |leaf| leaves.push(leaf));
            // The leaves from the start to the end: an end that does not show
            // (hidden, or in an unmounted row) by where it would be.
            let first = match &start {
                End::Live(point) => leaves.iter().position(|l| l.id == point.node).or_else(|| {
                    leaves
                        .iter()
                        .position(|l| scene.document_order(l.id, point.node).is_ge())
                }),
                End::Parked(row, _) => leaves
                    .iter()
                    .position(|l| row_order(rows, &scene, l.id, row).is_ge()),
                End::Gone => None,
            };
            let last = match &end {
                End::Live(point) => leaves.iter().position(|l| l.id == point.node).or_else(|| {
                    leaves
                        .iter()
                        .rposition(|l| scene.document_order(l.id, point.node).is_le())
                }),
                End::Parked(row, _) => leaves
                    .iter()
                    .rposition(|l| row_order(rows, &scene, l.id, row).is_le()),
                End::Gone => None,
            };
            let (first, last) = match (first, last) {
                (Some(first), Some(last)) if first <= last => (first, last),
                _ => (1, 0),
            };
            let (start, end) = (live(&start), live(&end));
            for leaf in leaves.get(first..=last).unwrap_or_default() {
                let Some(text) = text_of(&scene, leaf.id) else {
                    continue;
                };
                let from = match start {
                    Some(start) if leaf.id == start.node => boundary(&text, start.offset),
                    _ => 0,
                };
                let to = match end {
                    Some(end) if leaf.id == end.node => boundary(&text, end.offset),
                    _ => text.len(),
                };
                if from < to
                    && let Some(stamp) = stamp(&scene, leaf.id)
                {
                    shown.push(Shown {
                        node: leaf.id,
                        range: from..to,
                        stamp,
                    });
                }
            }
        }
        leaves.clear();
        state.leaves = leaves;
        // For assistive technology: the region's anchor and focus, in its
        // labels' values (`SemanticNode::document_selection`).
        let document = shown.first().zip(shown.last()).map(|(first, last)| {
            let scene = self.scene.borrow();
            let start = (
                first.node,
                semantic_offset(&scene, first.node, first.range.start),
            );
            let end = (
                last.node,
                semantic_offset(&scene, last.node, last.range.end),
            );
            if backwards {
                (end, start)
            } else {
                (start, end)
            }
        });
        let semantic_root = selected.filter(|_| document.is_some());
        let previous = std::mem::replace(&mut state.semantic_root, semantic_root);
        let old = std::mem::replace(&mut state.shown, shown.clone());
        let inactive = state.inactive;
        {
            let scene = self.scene.borrow();
            state.synced = (
                scene.content_revision(),
                scene.layout_revision(),
                scene.text_shaper_revision(),
            );
        }
        drop(state);
        {
            let mut semantics = self.semantics.borrow_mut();
            if let Some(previous) = previous.filter(|previous| Some(*previous) != semantic_root) {
                semantics.update(previous, |node| node.document_selection = None);
            }
            if let Some(root) = semantic_root {
                semantics.update(root, |node| node.document_selection = document);
            }
        }
        // Both run in document order, mostly the same nodes: what was shown
        // is looked up by node.
        let before: HashMap<NodeId, &Shown> = old.iter().map(|part| (part.node, part)).collect();
        let now: std::collections::HashSet<NodeId> = shown.iter().map(|part| part.node).collect();
        for gone in old.iter().filter(|o| !now.contains(&o.node)) {
            let mut scene = self.scene.borrow_mut();
            if scene.contains(gone.node) {
                scene.set_text_highlight(gone.node, None);
            }
        }
        let old_deferred = std::mem::take(&mut self.selection_state().borrow_mut().deferred);
        let Some(palette) = colors else {
            return;
        };
        let was_deferred: std::collections::HashSet<NodeId> =
            old_deferred.iter().map(|part| part.node).collect();
        let changed: Vec<Shown> = shown
            .iter()
            .filter(|part| {
                recolor || was_deferred.contains(&part.node) || before.get(&part.node) != Some(part)
            })
            .cloned()
            .collect();
        let deferred = self.lay_out_highlights(changed, palette, inactive);
        {
            let mut state = self.selection_state().borrow_mut();
            state.deferred = deferred;
            state.deferred_at = self.scene.borrow().geometry_revision();
        }
        let gesture = self.selection_state().borrow().gesture.is_some();
        if !gesture {
            self.selection_state().borrow_mut().layouts.clear();
        }
    }
    /// Lay out the highlights of `parts` in view; returns those out of
    /// view, their highlights cleared until they show.
    fn lay_out_highlights(
        &self,
        parts: Vec<Shown>,
        palette: Palette,
        inactive: bool,
    ) -> Vec<Shown> {
        let mut deferred = Vec::new();
        for part in parts {
            let shows = self
                .scene
                .borrow()
                .visible_bounds(part.node)
                .is_some_and(|r| r.width > 0. && r.height > 0.);
            if !shows {
                self.scene.borrow_mut().set_text_highlight(part.node, None);
                deferred.push(part);
                continue;
            }
            let rects = {
                let scene = self.scene.borrow();
                let bounds = scene.layout_bounds(part.node);
                let Some(layout) = self.selection_layout(&scene, part.node) else {
                    continue;
                };
                let frame = Rect::new(0., 0., bounds.width, bounds.height);
                layout
                    .selection(part.range.clone())
                    .into_iter()
                    .filter_map(|r| r.intersection(frame))
                    .collect()
            };
            self.scene
                .borrow_mut()
                .set_text_highlight(part.node, Some(palette.highlight(inactive, rects)));
        }
        deferred
    }
    /// Lay out the highlights of deferred parts that now show.
    fn reveal_deferred_highlights(&self) {
        let geometry = self.scene.borrow().geometry_revision();
        let (parts, palette, inactive) = {
            let mut state = self.selection_state().borrow_mut();
            if state.deferred.is_empty() || state.deferred_at == geometry {
                return;
            }
            state.deferred_at = geometry;
            let palette = state.root.and_then(|root| state.roots.get(&root).copied());
            let Some(palette) = palette else {
                return;
            };
            (std::mem::take(&mut state.deferred), palette, state.inactive)
        };
        let parts: Vec<Shown> = {
            let scene = self.scene.borrow();
            parts
                .into_iter()
                .filter(|part| scene.contains(part.node))
                .collect()
        };
        let deferred = self.lay_out_highlights(parts, palette, inactive);
        self.selection_state().borrow_mut().deferred = deferred;
    }
    /// Keep highlights on the text as it is laid out again, and drop a
    /// selection whose text is gone. Frame preparation calls this; it
    /// returns at once unless a selection shows and the scene changed.
    pub(crate) fn sync_selection(&self) {
        let synced = {
            let state = self.selection_state().borrow();
            if state.anchor.is_none() {
                return;
            }
            state.synced
        };
        let now = {
            let scene = self.scene.borrow();
            (
                scene.content_revision(),
                scene.layout_revision(),
                scene.text_shaper_revision(),
            )
        };
        if now == synced {
            self.reveal_deferred_highlights();
            return;
        }
        // Nothing added or removed: laid out again, the selected text keeps
        // its highlights unless its own size or text changed.
        if now.0 == synced.0 && now.2 == synced.2 {
            let mut state = self.selection_state().borrow_mut();
            let scene = self.scene.borrow();
            let same = state
                .shown
                .iter()
                .all(|part| stamp(&scene, part.node) == Some(part.stamp));
            if same {
                state.synced = now;
                drop(state);
                drop(scene);
                self.reveal_deferred_highlights();
                return;
            }
        }
        self.refresh_selection(false);
    }
    /// How fast a drag scrolls, in points a second along each axis: toward
    /// the pointer while it is held past an edge of the innermost scroll
    /// viewport holding the selection's focus, faster the farther it is.
    fn selection_scroll_velocity(&self) -> (f32, f32) {
        let state = self.selection_state().borrow();
        let Some(gesture) = &state.gesture else {
            return (0., 0.);
        };
        let scene = self.scene.borrow();
        let from = state
            .focus
            .map(|focus| focus.node)
            .filter(|node| scene.contains(*node))
            .unwrap_or(gesture.root);
        if !scene.contains(from) {
            return (0., 0.);
        }
        let Some(visible) = std::iter::once(from)
            .chain(scene.ancestors(from))
            .find(|id| scene.is_scroll_content(*id))
            .and_then(|content| scene.parent(content))
            .and_then(|viewport| scene.visible_bounds(viewport))
        else {
            return (0., 0.);
        };
        let speed = |at: f32, start: f32, extent: f32| {
            let past = if at < start {
                at - start
            } else if at > start + extent {
                at - start - extent
            } else {
                0.
            };
            if past == 0. || !past.is_finite() {
                0.
            } else {
                past.signum() * (past.abs() * 12.).clamp(24., 1200.)
            }
        };
        let (x, y) = gesture.pointer;
        (
            speed(x, visible.x, visible.width),
            speed(y, visible.y, visible.height),
        )
    }
    /// Schedule a drag's autoscroll while the pointer is past an edge.
    fn arm_selection_scroll(&self) {
        let moving = self.selection_scroll_velocity() != (0., 0.);
        let mut state = self.selection_state().borrow_mut();
        if !moving {
            state.scroll = None;
        } else if state.scroll.is_none() {
            let now = Instant::now();
            state.scroll = Some((now + Duration::from_millis(16), now));
        }
    }
    /// When a drag held past a scroll viewport's edge next scrolls it.
    pub(crate) fn next_selection_deadline(&self) -> Option<Instant> {
        let state = self.selection_state().borrow();
        state
            .scroll
            .filter(|_| state.gesture.is_some())
            .map(|(next, _)| next)
    }
    /// Scroll for a drag held past a viewport's edge, if due, and extend the
    /// selection over what scrolled under the pointer. A stall contributes
    /// at most 50 ms of motion; a viewport that cannot scroll further stops
    /// the ticks until the pointer moves again. Returns whether it scrolled.
    pub(crate) fn advance_selection_scroll(&self, now: Instant) -> bool {
        let (root, pointer, target, last) = {
            let state = self.selection_state().borrow();
            let Some((next, last)) = state.scroll else {
                return false;
            };
            let Some(gesture) = state.gesture.as_ref().filter(|_| next <= now) else {
                return false;
            };
            let target = state
                .focus
                .map(|focus| focus.node)
                .filter(|node| self.scene.borrow().contains(*node))
                .unwrap_or(gesture.root);
            (gesture.root, gesture.pointer, target, last)
        };
        let velocity = self.selection_scroll_velocity();
        let elapsed = now
            .saturating_duration_since(last)
            .min(Duration::from_millis(50))
            .as_secs_f32();
        let scrolled = velocity != (0., 0.)
            && self
                .input
                .dispatch_to(
                    &self.scene,
                    target,
                    InputEvent::Scroll {
                        x: pointer.0,
                        y: pointer.1,
                        delta_x: velocity.0 * elapsed,
                        delta_y: velocity.1 * elapsed,
                    },
                )
                .default_prevented;
        if !scrolled {
            self.selection_state().borrow_mut().scroll = None;
            return false;
        }
        // Lay the scrolled text out where it now is, then reach it.
        let _ = self.try_prepare_frame();
        self.drag_selection(root, pointer.0, pointer.1);
        if let Some(scroll) = &mut self.selection_state().borrow_mut().scroll {
            *scroll = (now + Duration::from_millis(16), now);
        }
        true
    }
    /// Whether a pointer gesture is making the selection now: a host
    /// publishes it once it settles (the primary selection).
    pub fn is_selecting_text(&self) -> bool {
        self.selection_state().borrow().gesture.is_some()
    }
    /// Whether window point `x`, `y` is on selected text.
    fn on_selected_text(&self, x: f32, y: f32) -> bool {
        let shown = self.selection_state().borrow().shown.clone();
        let scene = self.scene.borrow();
        shown.iter().any(|part| {
            let (Some(highlight), true) =
                (scene.text_highlight(part.node), scene.contains(part.node))
            else {
                return false;
            };
            let bounds = scene.bounds(part.node);
            let Some((x, y)) = scene.point_in(part.node, x, y) else {
                return false;
            };
            highlight.rects.iter().any(|r| {
                let (left, top) = (bounds.x + r.x, bounds.y + r.y);
                x >= left && x < left + r.width && y >= top && y < top + r.height
            })
        })
    }
    /// The selection's anchor and focus, if there is one (it may be
    /// collapsed: both at one place). An end in a virtual list's row that
    /// the list has unmounted names the node it had; the selection comes
    /// back with the row.
    pub fn text_selection(&self) -> Option<(TextPoint, TextPoint)> {
        let state = self.selection_state().borrow();
        state.anchor.zip(state.focus)
    }
    /// Whether some text is selected.
    pub fn has_text_selection(&self) -> bool {
        !self.selection_state().borrow().shown.is_empty()
    }
    /// Changes whenever the selection does: a host publishes it then (the
    /// X11 and Wayland primary selection, accessibility).
    pub fn text_selection_revision(&self) -> u64 {
        self.selection_state().borrow().revision
    }
    /// Select from `anchor` to `focus`, both in one selectable region.
    /// Returns false (selecting nothing) otherwise.
    pub fn select_text(&self, anchor: TextPoint, focus: TextPoint) -> bool {
        let root = {
            let scene = self.scene.borrow();
            let root = scene.selection_root(anchor.node);
            if root.is_none() || root != scene.selection_root(focus.node) {
                return false;
            }
            root
        };
        if !self
            .selection_state()
            .borrow()
            .roots
            .contains_key(&root.unwrap())
        {
            return false;
        }
        self.set_selection(root, Some(anchor), Some(focus));
        true
    }
    /// Select as assistive technology asks: each end a label and an offset
    /// into its semantic value (`SemanticNode::document_selection`).
    pub fn select_accessible_text(&self, anchor: (NodeId, usize), focus: (NodeId, usize)) -> bool {
        let point = |(node, offset): (NodeId, usize)| {
            let scene = self.scene.borrow();
            scene.contains(node).then(|| TextPoint {
                node,
                offset: scene_offset(&scene, node, offset),
            })
        };
        match (point(anchor), point(focus)) {
            (Some(anchor), Some(focus)) => self.select_text(anchor, focus),
            _ => false,
        }
    }
    /// Select all the text in the selectable region holding `node`.
    pub fn select_all_text(&self, node: NodeId) -> bool {
        let Some(root) = self.scene.borrow().selection_root(node) else {
            return false;
        };
        let (first, last) = self.region_ends(root);
        self.selection_state().borrow_mut().active_root = Some(root);
        if first.is_none() {
            return false;
        }
        self.set_selection(Some(root), first, last);
        self.selection_state().borrow_mut().all = true;
        true
    }
    /// The start of region `root`'s first selectable text and the end of
    /// its last.
    fn region_ends(&self, root: NodeId) -> (Option<TextPoint>, Option<TextPoint>) {
        let scene = self.scene.borrow();
        let (mut first, mut last) = (None, None);
        scene.selectable_text(
            root,
            |id| self.is_editor(id),
            |leaf| {
                first.get_or_insert(leaf.id);
                last = Some(leaf.id);
            },
        );
        let end = last
            .and_then(|id| text_of(&scene, id))
            .map_or(0, |text| text.len());
        (
            first.map(|node| TextPoint { node, offset: 0 }),
            last.map(|node| TextPoint { node, offset: end }),
        )
    }
    /// Select the word at window point `x`, `y`, as a double press there
    /// does: a long press on a touch screen starts a selection this way.
    /// Returns false (changing nothing) when no selectable text is there,
    /// or it is in an editor, which selects its own.
    pub fn select_word_at(&self, x: f32, y: f32) -> bool {
        let Some(target) = self.scene.borrow().hit_test(x, y) else {
            return false;
        };
        let Some(root) = self.scene.borrow().selection_root(target) else {
            return false;
        };
        if !self.selection_state().borrow().roots.contains_key(&root)
            || !self.selectable_target(root, target)
        {
            return false;
        }
        let Some(hit) = self.point_at(root, x, y) else {
            return false;
        };
        let (anchor, focus) = hit.edges(Unit::Word);
        {
            let mut state = self.selection_state().borrow_mut();
            state.active_root = Some(root);
            // During a press there (a finger held on the text: its press
            // began a gesture, or a control in the text holds it), moving on
            // extends the selection by words, as after a double press; the
            // release keeps it rather than clicking. A control gives the
            // pointer up at the next move.
            let control = state.pending.take_if(|p| p.root == root).is_some();
            let held = control || state.gesture.as_ref().is_some_and(|g| g.root == root);
            state.take_from = control.then_some(root);
            state.gesture = held.then_some(Gesture {
                root,
                unit: Unit::Word,
                edges: (anchor, focus),
                dragged: true,
                pointer: (x, y),
            });
        }
        self.set_selection(Some(root), Some(anchor), Some(focus));
        true
    }
    /// Clear the text selection.
    pub fn clear_text_selection(&self) {
        self.set_selection(None, None, None);
    }
    /// The selected text as plain text: a node's selected part as it is
    /// (soft wraps join, its own line breaks stay), and between nodes what
    /// their layout shows (see `separator`). Text that does not show is
    /// left out.
    pub fn selected_text(&self) -> String {
        let shown = self.selection_state().borrow().shown.clone();
        let provided = self.provided_rows(&shown);
        let scene = self.scene.borrow();
        let mut out = String::new();
        let mut previous: Option<(NodeId, f32)> = None;
        let mut provided = provided.into_iter().peekable();
        for (at, part) in shown.into_iter().enumerate() {
            // Rows a list has not mounted, from its row text, in order.
            while let Some((_, row)) = provided.next_if(|(before, _)| *before == at) {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(&row);
                previous = None;
            }
            if !scene.layout_visible(part.node) {
                continue;
            }
            if previous.is_none() && !out.is_empty() {
                out.push('\n');
            }
            let Some(text) = text_of(&scene, part.node) else {
                continue;
            };
            let size = font_size(&scene, part.node);
            if let Some((before, before_size)) = previous {
                out.push_str(separator(&scene, before, part.node, before_size.min(size)));
            }
            let range = part.range.start.min(text.len())..part.range.end.min(text.len());
            out.extend(
                text[range]
                    .chars()
                    .filter(|c| *c != '\u{2060}' && *c != '\u{fffc}'),
            );
            previous = Some((part.node, size));
        }
        for (_, row) in provided {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&row);
        }
        out
    }
    /// The text of the rows the selection spans in virtual lists with row
    /// text (`View::row_text`) that they have not mounted, each with the
    /// position in `shown` it comes before (`shown.len()`: after it all).
    /// Nothing without such lists.
    fn provided_rows(&self, shown: &[Shown]) -> Vec<(usize, String)> {
        let state = self.selection_state().borrow();
        if state.row_texts.is_empty() {
            return Vec::new();
        }
        let scene = self.scene.borrow();
        let rows = &state.rows;
        let end = |point: Option<TextPoint>, row: &Option<RowAddress>| match point {
            Some(point) => revive(rows, &scene, point, row.as_ref()),
            None => End::Gone,
        };
        let (anchor, focus) = (
            end(state.anchor, &state.anchor_row),
            end(state.focus, &state.focus_row),
        );
        if matches!(anchor, End::Gone) || matches!(focus, End::Gone) {
            return Vec::new();
        }
        let (start, end) = match order_ends(rows, &scene, &anchor, &focus).is_le() {
            true => (anchor, focus),
            false => (focus, anchor),
        };
        // Where an end lies against a list: its row there, or before (Less)
        // or after (Greater) the whole list.
        let place = |end: &End, content: NodeId| -> Result<usize, std::cmp::Ordering> {
            match end {
                End::Parked(row, _) if row.content == content => Ok(row.index),
                End::Parked(row, _) => Err(scene.document_order(row.content, content)),
                End::Live(point) => match row_of(rows, &scene, point.node) {
                    Some((at, index)) if at == content => Ok(index),
                    _ => Err(scene.document_order(point.node, content)),
                },
                End::Gone => Err(std::cmp::Ordering::Equal),
            }
        };
        let mut provided = Vec::new();
        // All of a region: every row of its lists.
        let all = state.root.filter(|_| state.all);
        for (content, text) in &state.row_texts {
            if !scene.contains(*content) {
                continue;
            }
            let (lo, hi) = match all {
                Some(root) if scene.ancestors(*content).any(|at| at == root) => (0, usize::MAX),
                Some(_) => continue,
                None => {
                    let lo = match place(&start, *content) {
                        Ok(index) => index,
                        Err(std::cmp::Ordering::Less) => 0,
                        Err(_) => continue,
                    };
                    let hi = match place(&end, *content) {
                        Ok(index) => index,
                        Err(std::cmp::Ordering::Greater) => usize::MAX,
                        Err(_) => continue,
                    };
                    (lo, hi)
                }
            };
            // A mounted row an end lies in copies as it shows, which may be
            // none of it (the selection collapsed there, or ending at its
            // start); one between the ends that shows no text (drawn, not
            // written) takes its row text.
            let in_row = |end: &End| match end {
                End::Live(point) => row_of(rows, &scene, point.node)
                    .filter(|(list, _)| list == content)
                    .map(|(_, index)| index),
                End::Parked(row, _) if row.content == *content => Some(row.index),
                _ => None,
            };
            let ends = [in_row(&start), in_row(&end)];
            let mounted_ends: std::collections::HashSet<usize> = scene
                .children(*content)
                .iter()
                .filter_map(|row| rows.get(row).map(|index| index.with_untracked(|i| *i)))
                .filter(|index| ends.contains(&Some(*index)))
                .collect();
            // The rows shown here (mounted), by index.
            let mounted: Vec<(usize, usize)> = shown
                .iter()
                .enumerate()
                .filter_map(|(at, part)| match row_of(rows, &scene, part.node) {
                    Some((list, index)) if list == *content => Some((at, index)),
                    _ => None,
                })
                .collect();
            let after_list = shown
                .iter()
                .position(|part| scene.document_order(part.node, *content).is_gt())
                .unwrap_or(shown.len());
            // Bounded, should a list never end.
            let hi = hi.min(lo.saturating_add(MAX_PROVIDED_ROWS));
            // An end in a row scrolled away cuts it where it did.
            let cut = |end: &End, index: usize| match end {
                End::Parked(row, _) if row.content == *content && row.index == index => {
                    row.copied.clone()
                }
                _ => None,
            };
            let mut index = lo;
            while index <= hi {
                if !mounted_ends.contains(&index) && !mounted.iter().any(|(_, m)| *m == index) {
                    let row = match (cut(&start, index), cut(&end, index)) {
                        (Some((row, from)), Some((_, to))) if from <= to && to <= row.len() => {
                            row[from..to].to_owned()
                        }
                        (Some((row, from)), None) => row[from.min(row.len())..].to_owned(),
                        (None, Some((row, to))) => row[..to.min(row.len())].to_owned(),
                        _ => match text(index) {
                            Some(row) => row,
                            None => break,
                        },
                    };
                    let before = mounted
                        .iter()
                        .find(|(_, m)| *m > index)
                        .map(|(at, _)| *at)
                        .or_else(|| mounted.last().map(|(at, _)| at + 1))
                        .unwrap_or(after_list);
                    provided.push((before, *content, row));
                }
                index += 1;
            }
        }
        // Where they go, then lists in document order (two lists with no
        // text shown between them take one place); stable, so rows of one
        // list keep theirs.
        provided.sort_by(|(a, list_a, _), (b, list_b, _)| {
            a.cmp(b)
                .then_with(|| scene.document_order(*list_a, *list_b))
        });
        provided
            .into_iter()
            .map(|(before, _, row)| (before, row))
            .collect()
    }
    /// The selected text as HTML, for pasting into rich text: paragraphs,
    /// headings (`<h1>`…) and line breaks where `selected_text` has them;
    /// bold, italic, monospace (code), underline and strikethrough runs;
    /// links with their targets; and the rows of virtual lists with row
    /// text that they have not mounted, as `selected_text` copies them.
    pub fn selected_html(&self) -> String {
        let shown = self.selection_state().borrow().shown.clone();
        let provided = self.provided_rows(&shown);
        let state = self.selection_state().borrow();
        let scene = self.scene.borrow();
        let semantics = self.semantics.borrow();
        // The block a text node is in: a heading's level, or a paragraph.
        let heading = |id: NodeId| -> Option<usize> {
            std::iter::once(id)
                .chain(scene.ancestors(id))
                .take_while(|at| state.root != Some(*at))
                .find_map(|at| {
                    semantics
                        .get(at)
                        .filter(|n| n.role == Role::Heading)
                        .map(|n| n.level.unwrap_or(1).clamp(1, 6))
                })
        };
        let tag = |level: Option<usize>| match level {
            Some(level) => format!("h{level}"),
            None => "p".to_owned(),
        };
        let mut out = String::from("<meta charset=\"utf-8\">");
        // The block open now, if any.
        let mut block: Option<Option<usize>> = None;
        let enter = |out: &mut String, block: &mut Option<Option<usize>>, level| {
            if let Some(open) = block.take() {
                out.push_str(&format!("</{}>", tag(open)));
            }
            out.push_str(&format!("<{}>", tag(level)));
            *block = Some(level);
        };
        let mut previous: Option<(NodeId, f32)> = None;
        let mut provided = provided.into_iter().peekable();
        let row = |out: &mut String, block: &mut Option<Option<usize>>, text: &str| {
            match block {
                Some(_) => out.push_str("<br>"),
                None => enter(out, block, None),
            }
            html_run(out, text, &FontStyle::default(), None, false, false);
        };
        for (at, part) in shown.into_iter().enumerate() {
            while let Some((_, text)) = provided.next_if(|(before, _)| *before == at) {
                row(&mut out, &mut block, &text);
                previous = None;
            }
            if !scene.layout_visible(part.node) {
                continue;
            }
            let size = font_size(&scene, part.node);
            let level = heading(part.node);
            match (block, previous) {
                (Some(open), Some((before, before_size))) if open == level => {
                    match separator(&scene, before, part.node, before_size.min(size)) {
                        "\t" => out.push(' '),
                        "\n\n" => enter(&mut out, &mut block, level),
                        _ => out.push_str("<br>"),
                    }
                }
                (Some(open), None) if open == level => out.push_str("<br>"),
                _ => enter(&mut out, &mut block, level),
            }
            let links = state.links.get(&part.node);
            match scene.kind(part.node) {
                NodeKind::Text { text, .. } if !scene.font(part.node).masked => {
                    let range = part.range.start.min(text.len())..part.range.end.min(text.len());
                    html_run(
                        &mut out,
                        &text[range],
                        scene.font(part.node),
                        None,
                        false,
                        false,
                    );
                }
                NodeKind::RichText { text } => {
                    for run in text.runs() {
                        let start = run.range.start.max(part.range.start);
                        let end = run.range.end.min(part.range.end);
                        if start < end {
                            let href = links.and_then(|links| {
                                links
                                    .iter()
                                    .find(|(range, _)| range.start <= start && end <= range.end)
                                    .map(|(_, href)| &**href)
                            });
                            html_run(
                                &mut out,
                                &text.text()[start..end],
                                &run.font,
                                href,
                                run.underline.is_some(),
                                run.strikethrough.is_some(),
                            );
                        }
                    }
                }
                _ => {}
            }
            previous = Some((part.node, size));
        }
        for (_, text) in provided {
            row(&mut out, &mut block, &text);
        }
        match block {
            Some(open) => out.push_str(&format!("</{}>", tag(open))),
            None => out.push_str("<p></p>"),
        }
        out
    }
    /// The window was activated or sent to the back: selections show the
    /// active or inactive colour.
    pub fn set_window_active(&self, active: bool) {
        let changed = {
            let mut state = self.selection_state().borrow_mut();
            let changed = state.inactive == active;
            state.inactive = !active;
            changed && !state.shown.is_empty()
        };
        if changed {
            self.refresh_selection(true);
        }
    }
    /// Move the selection's focus by `step` from the keyboard, keeping its
    /// anchor, as Shift with an arrow, Home or End does on a web page: by a
    /// grapheme, a word, to the visual line's edge, a line up or down, or to
    /// the region's start or end, from one text node into the next. Returns
    /// whether it moved (there was a selection to extend).
    fn extend_selection(&self, step: Step) -> bool {
        let (root, anchor, focus) = {
            let state = self.selection_state().borrow();
            let (Some(root), Some(anchor), Some(focus)) = (state.root, state.anchor, state.focus)
            else {
                return false;
            };
            (root, anchor, focus)
        };
        let next = {
            let scene = self.scene.borrow();
            if !scene.contains(root) || !scene.contains(focus.node) {
                return false;
            }
            let mut leaves = Vec::new();
            scene.selectable_text(root, |id| self.is_editor(id), |leaf| leaves.push(leaf));
            let Some(at) = leaves.iter().position(|leaf| leaf.id == focus.node) else {
                return false;
            };
            let text = |index: usize| text_of(&scene, leaves[index].id).unwrap_or_default();
            let point = |index: usize, offset: usize| TextPoint {
                node: leaves[index].id,
                offset,
            };
            let own = text(at);
            let offset = boundary(&own, focus.offset);
            match step {
                Step::Character(forward) | Step::Word(forward) => {
                    let word = matches!(step, Step::Word(_));
                    let within = if forward {
                        next_boundary(&own, offset, word)
                    } else {
                        previous_boundary(&own, offset, word)
                    };
                    // Past a node's edge: into the next text, as over a line
                    // break between blocks.
                    match within {
                        Some(offset) => point(at, offset),
                        None if forward && at + 1 < leaves.len() => {
                            let next = text(at + 1);
                            match word {
                                true => point(at + 1, next_boundary(&next, 0, true).unwrap_or(0)),
                                false => point(at + 1, 0),
                            }
                        }
                        None if !forward && at > 0 => {
                            let before = text(at - 1);
                            let end = before.len();
                            match word {
                                true => point(
                                    at - 1,
                                    previous_boundary(&before, end, true).unwrap_or(end),
                                ),
                                false => point(at - 1, end),
                            }
                        }
                        None => point(at, offset),
                    }
                }
                Step::Document(forward) => match forward {
                    true => point(leaves.len() - 1, text(leaves.len() - 1).len()),
                    false => point(0, 0),
                },
                Step::LineEdge(forward) | Step::Line(forward) => {
                    let Some(layout) = self.selection_layout(&scene, focus.node) else {
                        return false;
                    };
                    let position = crate::text_layout::TextPosition {
                        byte_offset: offset,
                        affinity: crate::text_layout::TextAffinity::After,
                    };
                    if matches!(step, Step::LineEdge(_)) {
                        point(at, layout.visual_line_edge(position, forward).byte_offset)
                    } else {
                        // The point a line above or below, where the caret is:
                        // it may be in another text node.
                        let caret = layout.caret_position(position);
                        let bounds = scene.bounds(focus.node);
                        let x = bounds.x + caret.x;
                        let y = bounds.y
                            + caret.y
                            + if forward {
                                caret.height * 1.5
                            } else {
                                -caret.height * 0.5
                            };
                        drop(scene);
                        let hit = self.point_near(root, x, y, false).map(|hit| hit.point);
                        let Some(hit) = hit else {
                            return false;
                        };
                        hit
                    }
                }
            }
        };
        self.selection_state().borrow_mut().active_root = Some(root);
        self.set_selection(Some(root), Some(anchor), Some(next));
        self.reveal_selection_focus();
        true
    }
    /// Scroll the innermost viewport holding the focus until the focus's
    /// line shows, as a page does when Shift-keys extend past its edge.
    fn reveal_selection_focus(&self) {
        let Some(focus) = self.selection_state().borrow().focus else {
            return;
        };
        let delta = {
            let scene = self.scene.borrow();
            if !scene.contains(focus.node) {
                return;
            }
            let Some(layout) = self.selection_layout(&scene, focus.node) else {
                return;
            };
            let caret = layout.caret_position(crate::text_layout::TextPosition {
                byte_offset: focus.offset,
                affinity: crate::text_layout::TextAffinity::After,
            });
            let bounds = scene.bounds(focus.node);
            let (top, bottom) = (bounds.y + caret.y, bounds.y + caret.y + caret.height);
            let Some(visible) = scene
                .ancestors(focus.node)
                .find(|id| scene.is_scroll_content(*id))
                .and_then(|content| scene.parent(content))
                .and_then(|viewport| scene.visible_bounds(viewport))
            else {
                return;
            };
            if top < visible.y {
                top - visible.y
            } else if bottom > visible.y + visible.height {
                bottom - visible.y - visible.height
            } else {
                return;
            }
        };
        self.input.dispatch_to(
            &self.scene,
            focus.node,
            InputEvent::Scroll {
                x: 0.,
                y: 0.,
                delta_x: 0.,
                delta_y: delta,
            },
        );
    }
    /// What the selection makes of an event the document has dispatched:
    /// select-all and the end of a selection on a press elsewhere.
    pub(crate) fn selection_after_dispatch(
        &self,
        event: SelectionEvent,
        result: &mut crate::input::DispatchResult,
    ) {
        match event {
            SelectionEvent::None => {}
            SelectionEvent::Extend(step) => {
                if !result.default_prevented
                    && self.focused_editor().is_none()
                    && self.extend_selection(step)
                {
                    result.default_prevented = true;
                }
            }
            SelectionEvent::SelectAll => {
                if result.default_prevented
                    || self.selection_state().borrow().roots.is_empty()
                    || self.focused_editor().is_some()
                {
                    return;
                }
                let root = {
                    let state = self.selection_state().borrow();
                    state.root.or(state.active_root)
                };
                if let Some(root) = root.filter(|root| self.scene.borrow().contains(*root))
                    && self.select_all_text(root)
                {
                    result.default_prevented = true;
                }
            }
            SelectionEvent::Press => {
                {
                    let mut state = self.selection_state().borrow_mut();
                    if std::mem::take(&mut state.pressed)
                        || (state.anchor.is_none() && state.active_root.is_none())
                    {
                        return;
                    }
                }
                // Elsewhere: on a control that acts (a button, a link, a menu
                // item, as a copy button would be) the selection stays, as on a
                // page; anywhere else it ends.
                let acts = result.target.is_some_and(|target| {
                    let scene = self.scene.borrow();
                    let semantics = self.semantics.borrow();
                    scene.contains(target)
                        && std::iter::once(target)
                            .chain(scene.ancestors(target))
                            .find_map(|id| semantics.get(id).map(|node| node.role))
                            .is_some_and(|role| {
                                matches!(
                                    role,
                                    Role::Button | Role::Link | Role::MenuItem | Role::CheckBox
                                )
                            })
                });
                if !acts {
                    self.selection_state().borrow_mut().active_root = None;
                    if self.selection_state().borrow().anchor.is_some() {
                        self.clear_text_selection();
                    }
                }
            }
        }
    }
}

/// A selectable region's context menu: Copy and Select All, on selected
/// text (elsewhere the press goes on to an enclosing view's menu).
pub(crate) fn context_menu(
    ui: &Ui,
    root: NodeId,
    platform: Option<Rc<crate::platform::PlatformService>>,
) -> crate::compose_context_menu::ContextMenuBuilder {
    let weak = ui.downgrade();
    Box::new(move |event| {
        let Some(ui) = weak.upgrade() else {
            return Vec::new();
        };
        if ui.selection_state().borrow().root != Some(root)
            || !ui.on_selected_text(event.x, event.y)
        {
            return Vec::new();
        }
        let (copy, all, look) = (weak.clone(), weak.clone(), weak.clone());
        let platform = platform.clone();
        // Look Up first, as AppKit's text menus have it, where the
        // platform looks words up.
        let mut items = Vec::new();
        if let Some(platform) = platform.clone().filter(|p| p.0.looks_up()) {
            items.push(
                crate::compose::menu_item(look_up_label(&ui.selected_text()))
                    .id("selection-look-up")
                    .on_click(move || {
                        if let Some(ui) = look.upgrade() {
                            ui.look_up_selection(&platform);
                        }
                    }),
            );
        }
        items.extend([
            crate::compose::menu_item("Copy")
                .id("selection-copy")
                .on_click(move || {
                    if let (Some(ui), Some(platform)) = (copy.upgrade(), &platform) {
                        platform
                            .0
                            .set_clipboard_rich_text(ui.selected_text(), ui.selected_html());
                    }
                }),
            crate::compose::menu_item("Select All")
                .id("selection-select-all")
                .on_click(move || {
                    if let Some(ui) = all.upgrade() {
                        ui.select_all_text(root);
                    }
                }),
        ]);
        items
    })
}

/// A Look Up item's label, as AppKit's: the text looked up, in quotes, cut
/// short.
pub(crate) fn look_up_label(text: &str) -> String {
    let text = text.trim();
    let mut shown: String = text.chars().take(24).collect();
    if shown.len() < text.len() {
        shown.push('…');
    }
    format!("Look Up “{shown}”")
}

impl Ui {
    /// Show the platform's definition of the selected display text over it
    /// (Look Up, ⌃⌘D on macOS); whether it did.
    pub fn look_up_selection(&self, platform: &crate::platform::PlatformService) -> bool {
        let text = self.selected_text();
        if text.trim().is_empty() {
            return false;
        }
        let start = {
            let state = self.selection_state().borrow();
            let (Some(anchor), Some(focus)) = (state.anchor, state.focus) else {
                return false;
            };
            let scene = self.scene.borrow();
            if !scene.contains(anchor.node) || !scene.contains(focus.node) {
                return false;
            }
            if order(&scene, anchor, focus).is_le() {
                anchor
            } else {
                focus
            }
        };
        let Some((at, size)) = self.scene.borrow().text_baseline(start.node, start.offset) else {
            return false;
        };
        platform.0.look_up(&text, at, size)
    }
}

/// What `Ui`'s dispatch looks for after the document has had an event.
#[derive(Clone, Copy)]
pub(crate) enum SelectionEvent {
    None,
    SelectAll,
    Press,
    /// Shift with an arrow, Home or End: move the focus.
    Extend(Step),
}
/// How far a Shift-key moves the focus, and which way (`true`: forward).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    Character(bool),
    Word(bool),
    LineEdge(bool),
    Line(bool),
    Document(bool),
}
/// A primary press that macOS takes as a secondary one (a control-click).
fn context_click(modifiers: Modifiers) -> bool {
    cfg!(target_os = "macos") && modifiers.control
}

impl SelectionEvent {
    pub(crate) fn of(event: &InputEvent, modifiers: Modifiers) -> Self {
        match event {
            InputEvent::KeyDown {
                key: Key::Character(key),
                modifiers,
                ..
            } if modifiers.primary_shortcut()
                && !modifiers.shift
                && !modifiers.alt
                && key.eq_ignore_ascii_case("a") =>
            {
                Self::SelectAll
            }
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                ..
            } if !context_click(modifiers) => Self::Press,
            InputEvent::KeyDown { key, modifiers, .. } if modifiers.shift => {
                let mac = cfg!(target_os = "macos");
                let step = match key {
                    Key::ArrowLeft | Key::ArrowRight => {
                        let forward = *key == Key::ArrowRight;
                        if mac && modifiers.meta {
                            Step::LineEdge(forward)
                        } else if modifiers.word_navigation() {
                            Step::Word(forward)
                        } else {
                            Step::Character(forward)
                        }
                    }
                    Key::ArrowUp | Key::ArrowDown => {
                        let forward = *key == Key::ArrowDown;
                        if mac && modifiers.meta {
                            Step::Document(forward)
                        } else {
                            Step::Line(forward)
                        }
                    }
                    Key::Home | Key::End => {
                        let forward = *key == Key::End;
                        if modifiers.primary_shortcut() {
                            Step::Document(forward)
                        } else {
                            Step::LineEdge(forward)
                        }
                    }
                    _ => return Self::None,
                };
                Self::Extend(step)
            }
            _ => Self::None,
        }
    }
}

/// What a node's highlight was laid out from (`Shown::stamp`).
fn stamp(scene: &Scene, id: NodeId) -> Option<(u32, u32, usize, usize, u64)> {
    if !scene.contains(id) {
        return None;
    }
    let text = match scene.kind(id) {
        NodeKind::Text { text, .. } => text.as_ref(),
        NodeKind::RichText { text } => text.text(),
        _ => return None,
    };
    let bounds = scene.layout_bounds(id);
    Some((
        bounds.width.to_bits(),
        bounds.height.to_bits(),
        text.as_ptr() as usize,
        text.len(),
        scene.text_shaper_revision(),
    ))
}

fn shape(scene: &Scene, id: NodeId, width: f32) -> Option<Rc<dyn TextLayout>> {
    // Renderers lay text out at its box's width, wrapping or not.
    Some(match scene.kind(id) {
        NodeKind::Text {
            text, font_size, ..
        } => scene
            .shape_text_with_font(text, *font_size, Some(width), scene.font(id))
            .into(),
        NodeKind::RichText { text } => scene.shape_rich_text(text, Some(width)).into(),
        _ => return None,
    })
}

fn text_of(scene: &Scene, id: NodeId) -> Option<Arc<str>> {
    match scene.kind(id) {
        NodeKind::Text { text, .. } if !scene.font(id).masked => Some(text.clone()),
        NodeKind::RichText { text } => Some(text.text_arc()),
        _ => None,
    }
}

/// What goes between the text of `a` and of `b`, after it, in copied text,
/// from the boxes that hold them side by side in their nearest shared
/// ancestor (as CSS's margins set blocks apart): a tab between boxes on one
/// row, a blank line between boxes set apart by half the text's size (a
/// paragraph's spacing) or more, else a line break.
fn separator(scene: &Scene, a: NodeId, b: NodeId, size: f32) -> &'static str {
    let path = |id: NodeId| {
        let mut path: Vec<NodeId> = std::iter::once(id).chain(scene.ancestors(id)).collect();
        path.reverse();
        path
    };
    let (pa, pb) = (path(a), path(b));
    let shared = pa.iter().zip(&pb).take_while(|(x, y)| x == y).count();
    let (Some(a), Some(b)) = (pa.get(shared), pb.get(shared)) else {
        return "\n";
    };
    let (a, b) = (scene.bounds(*a), scene.bounds(*b));
    if b.y < a.y + a.height * 0.5 && b.x >= a.x + a.width - 1. {
        "\t"
    } else if b.y - (a.y + a.height) >= size * 0.5 {
        "\n\n"
    } else {
        "\n"
    }
}

fn font_size(scene: &Scene, id: NodeId) -> f32 {
    match scene.kind(id) {
        NodeKind::Text { font_size, .. } => *font_size,
        NodeKind::RichText { text } => text.runs().first().map_or(16., |run| run.font_size),
        _ => 16.,
    }
}

/// `offset` into `id`'s text as an offset into its semantic value, which
/// leaves inline icons' placeholders out.
fn semantic_offset(scene: &Scene, id: NodeId, offset: usize) -> usize {
    match scene.kind(id) {
        NodeKind::RichText { text } => {
            let icon = crate::compose_rich::ICON_PLACEHOLDER;
            offset
                - text.text()[..offset.min(text.text().len())]
                    .matches(icon)
                    .count()
                    * icon.len()
        }
        _ => offset,
    }
}

/// The reverse of `semantic_offset`.
fn scene_offset(scene: &Scene, id: NodeId, semantic: usize) -> usize {
    let NodeKind::RichText { text } = scene.kind(id) else {
        return semantic;
    };
    let icon = crate::compose_rich::ICON_PLACEHOLDER;
    let (mut seen, mut offset) = (0, 0);
    let text = text.text();
    while offset < text.len() {
        if text[offset..].starts_with(icon) {
            offset += icon.len();
            continue;
        }
        if seen >= semantic {
            break;
        }
        let step = text[offset..].chars().next().map_or(1, char::len_utf8);
        seen += step;
        offset += step;
    }
    offset
}

/// One styled run of `selected_html`: escaped, with its line breaks.
fn html_run(
    out: &mut String,
    text: &str,
    font: &FontStyle,
    href: Option<&str>,
    underline: bool,
    strike: bool,
) {
    if let Some(href) = href {
        out.push_str("<a href=\"");
        escape_html(out, href);
        out.push_str("\">");
    }
    let tags = [
        (font.weight >= 600, "b"),
        (font.italic, "i"),
        (
            font.family == crate::text_layout::FontFamily::Monospace,
            "code",
        ),
        // A link's underline is the link's.
        (
            href.is_none() && (underline || font.underline.is_some()),
            "u",
        ),
        (strike, "s"),
    ];
    for (_, tag) in tags.iter().filter(|(on, _)| *on) {
        out.push_str(&format!("<{tag}>"));
    }
    escape_html(out, text);
    for (_, tag) in tags.iter().rev().filter(|(on, _)| *on) {
        out.push_str(&format!("</{tag}>"));
    }
    if href.is_some() {
        out.push_str("</a>");
    }
}
/// `text` escaped for HTML, its line breaks `<br>`.
fn escape_html(out: &mut String, text: &str) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\n' => out.push_str("<br>"),
            '\u{2060}' | '\u{fffc}' => {}
            c => out.push(c),
        }
    }
}

/// The next grapheme boundary (or word end) after `offset` in `text`.
fn next_boundary(text: &str, offset: usize, word: bool) -> Option<usize> {
    use unicode_segmentation::UnicodeSegmentation;
    let rest = &text[offset..];
    if word {
        rest.unicode_word_indices()
            .next()
            .map(|(i, w)| offset + i + w.len())
    } else {
        rest.graphemes(true).next().map(|g| offset + g.len())
    }
}

/// The grapheme boundary (or word start) before `offset` in `text`.
fn previous_boundary(text: &str, offset: usize, word: bool) -> Option<usize> {
    use unicode_segmentation::UnicodeSegmentation;
    let before = &text[..offset];
    if word {
        before.unicode_word_indices().map(|(i, _)| i).next_back()
    } else {
        before.grapheme_indices(true).map(|(i, _)| i).next_back()
    }
}

/// `offset` within `text`, on a character boundary at or before it.
fn boundary(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

/// The virtual list `node`'s row is in (the node holding its rows) and
/// the row's index.
fn row_of(
    rows: &HashMap<NodeId, crate::reactive::Signal<usize>>,
    scene: &Scene,
    node: NodeId,
) -> Option<(NodeId, usize)> {
    if rows.is_empty() || !scene.contains(node) {
        return None;
    }
    std::iter::once(node)
        .chain(scene.ancestors(node))
        .find_map(|at| Some((scene.parent(at)?, rows.get(&at)?.with_untracked(|i| *i))))
}

/// Where `point` lies in a virtual list's row, as a `RowAddress`.
fn row_address(
    rows: &HashMap<NodeId, crate::reactive::Signal<usize>>,
    scene: &Scene,
    point: TextPoint,
) -> Option<RowAddress> {
    if rows.is_empty() || !scene.contains(point.node) {
        return None;
    }
    let mut path = Vec::new();
    let mut at = point.node;
    while let Some(parent) = scene.parent(at) {
        if let Some(index) = rows.get(&at) {
            path.reverse();
            return Some(RowAddress {
                content: parent,
                index: index.with_untracked(|index| *index),
                path,
                copied: row_copy(scene, at, point),
            });
        }
        path.push(scene.children(parent).iter().position(|c| *c == at)? as u32);
        at = parent;
    }
    None
}

/// Row `row`'s text as `selected_text` takes it (its text nodes joined as
/// a copy joins them), and where `point` lies in it.
fn row_copy(scene: &Scene, row: NodeId, point: TextPoint) -> Option<(Rc<str>, usize)> {
    let clean = |text: &str, out: &mut String| {
        out.extend(
            text.chars()
                .filter(|c| *c != '\u{2060}' && *c != '\u{fffc}'),
        );
    };
    let (mut out, mut at) = (String::new(), None);
    let mut previous: Option<(NodeId, f32)> = None;
    let mut pending = vec![row];
    while let Some(id) = pending.pop() {
        pending.extend(scene.children(id).iter().rev());
        if !scene.layout_visible(id) {
            continue;
        }
        let Some(text) = text_of(scene, id) else {
            continue;
        };
        let size = font_size(scene, id);
        if let Some((before, before_size)) = previous {
            out.push_str(separator(scene, before, id, before_size.min(size)));
        }
        if id == point.node {
            let cut = point.offset.min(text.len());
            clean(&text[..cut], &mut out);
            at = Some(out.len());
            clean(&text[cut..], &mut out);
        } else {
            clean(&text, &mut out);
        }
        previous = Some((id, size));
    }
    Some((out.into(), at?))
}

/// The text node at `row`, if its row is mounted.
fn resolve_row(
    rows: &HashMap<NodeId, crate::reactive::Signal<usize>>,
    scene: &Scene,
    row: &RowAddress,
) -> Option<NodeId> {
    let mounted = scene.children(row.content).iter().find(|child| {
        rows.get(child)
            .is_some_and(|index| index.with_untracked(|index| *index) == row.index)
    })?;
    let mut at = *mounted;
    for index in &row.path {
        at = *scene.children(at).get(*index as usize)?;
    }
    matches!(
        scene.kind(at),
        NodeKind::Text { .. } | NodeKind::RichText { .. }
    )
    .then_some(at)
}

/// `point` as it stands, found again in its row if that was recycled.
fn revive(
    rows: &HashMap<NodeId, crate::reactive::Signal<usize>>,
    scene: &Scene,
    point: TextPoint,
    row: Option<&RowAddress>,
) -> End {
    if scene.contains(point.node) {
        return End::Live(point);
    }
    let Some(row) = row.filter(|row| scene.contains(row.content)) else {
        return End::Gone;
    };
    match resolve_row(rows, scene, row) {
        Some(node) => End::Live(TextPoint {
            node,
            offset: point.offset,
        }),
        None => End::Parked(row.clone(), point.offset),
    }
}

fn live(end: &End) -> Option<TextPoint> {
    match end {
        End::Live(point) => Some(*point),
        _ => None,
    }
}

/// Where text node `leaf` lies against an unmounted row's place.
fn row_order(
    rows: &HashMap<NodeId, crate::reactive::Signal<usize>>,
    scene: &Scene,
    leaf: NodeId,
    row: &RowAddress,
) -> Ordering {
    let mut at = leaf;
    while let Some(parent) = scene.parent(at) {
        if parent == row.content {
            return rows.get(&at).map_or(Ordering::Equal, |index| {
                index.with_untracked(|index| *index).cmp(&row.index)
            });
        }
        at = parent;
    }
    scene.document_order(leaf, row.content)
}

fn order_ends(
    rows: &HashMap<NodeId, crate::reactive::Signal<usize>>,
    scene: &Scene,
    a: &End,
    b: &End,
) -> Ordering {
    match (a, b) {
        (End::Live(a), End::Live(b)) => order(scene, *a, *b),
        (End::Live(a), End::Parked(row, offset)) => {
            row_order(rows, scene, a.node, row).then(a.offset.cmp(offset))
        }
        (End::Parked(row, offset), End::Live(b)) => row_order(rows, scene, b.node, row)
            .reverse()
            .then(offset.cmp(&b.offset)),
        (End::Parked(a, x), End::Parked(b, y)) if a.content == b.content => a
            .index
            .cmp(&b.index)
            .then(a.path.cmp(&b.path))
            .then(x.cmp(y)),
        (End::Parked(a, _), End::Parked(b, _)) => scene.document_order(a.content, b.content),
        _ => Ordering::Equal,
    }
}

fn order(scene: &Scene, a: TextPoint, b: TextPoint) -> std::cmp::Ordering {
    scene
        .document_order(a.node, b.node)
        .then(a.offset.cmp(&b.offset))
}
