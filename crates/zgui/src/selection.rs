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
use std::{collections::HashMap, ops::Range, rc::Rc, sync::Arc};

/// A place in selectable text: a text node and a UTF-8 byte offset into
/// its text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextPoint {
    pub node: NodeId,
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
                    &self.text,
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
    roots: HashMap<NodeId, (Color, Color)>,
    root: Option<NodeId>,
    anchor: Option<TextPoint>,
    focus: Option<TextPoint>,
    shown: Vec<Shown>,
    layouts: Vec<Shaped>,
    gesture: Option<Gesture>,
    pending: Option<Pending>,
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
        }
    }
}

/// Make `root` a selectable region (`View::selectable(true)`).
pub(crate) fn mount_root(ui: &mut Ui, root: NodeId) {
    ui.scene.borrow_mut().set_user_select(root, Some(true));
    let theme = ui.theme_scope();
    let weak = ui.downgrade();
    ui.bind(root, move || {
        let colors = (
            theme.color(tok::SELECTION),
            theme.color(tok::SELECTION_INACTIVE),
        );
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
                if cx.default_prevented() || !ui.selectable_target(root, cx.target) {
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
                let pending = ui.selection_state().borrow().pending;
                if let Some(pending) = pending.filter(|p| p.root == root) {
                    let (dx, dy) = (x - pending.x, y - pending.y);
                    if dx * dx + dy * dy <= 16. {
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
            (gesture.unit, gesture.edges)
        };
        let Some(hit) = self.point_at(root, x, y) else {
            return true;
        };
        let (anchor, focus) = if unit == Unit::Character {
            (edges.0, hit.point)
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
        let scene = self.scene.borrow();
        let mut best: Option<(TextLeaf, (f32, f32))> = None;
        scene.selectable_text(
            root,
            |id| self.is_editor(id),
            |leaf| {
                let Some(visible) = leaf.visible else {
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
                if best.is_none_or(|(_, best)| key <= best) {
                    best = Some((leaf, key));
                }
            },
        );
        let (leaf, _) = best?;
        let (bounds, visible) = (leaf.bounds, leaf.visible.unwrap());
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
            state.root = root;
            state.anchor = anchor;
            state.focus = focus;
            state.revision = state.revision.wrapping_add(1);
        }
        self.refresh_selection(false);
    }
    /// Lay the selection's highlights out again: only the text whose part of
    /// it, or whose layout, changed (all of it if `recolor`).
    pub(crate) fn refresh_selection(&self, recolor: bool) {
        let mut state = self.selection_state().borrow_mut();
        let selected =
            state
                .root
                .zip(state.anchor)
                .zip(state.focus)
                .filter(|((root, anchor), focus)| {
                    let scene = self.scene.borrow();
                    scene.contains(*root)
                        && scene.contains(anchor.node)
                        && scene.contains(focus.node)
                });
        // Its text is gone: so is the selection.
        if selected.is_none() && state.anchor.is_some() {
            state.root = None;
            state.anchor = None;
            state.focus = None;
            state.revision = state.revision.wrapping_add(1);
        }
        let mut shown = Vec::new();
        let mut leaves = std::mem::take(&mut state.leaves);
        let colors = selected.and_then(|((root, _), _)| state.roots.get(&root).copied());
        if let (Some(((root, anchor), focus)), Some(_)) = (selected, colors) {
            let scene = self.scene.borrow();
            let (start, end) = if order(&scene, anchor, focus).is_le() {
                (anchor, focus)
            } else {
                (focus, anchor)
            };
            leaves.clear();
            scene.selectable_text(root, |id| self.is_editor(id), |leaf| leaves.push(leaf));
            let index = |id: NodeId| leaves.iter().position(|leaf| leaf.id == id);
            let (first, last) = match (index(start.node), index(end.node)) {
                (Some(first), Some(last)) => (first, last),
                // An end that no longer shows (hidden): the leaves between.
                _ => {
                    let first = leaves
                        .iter()
                        .position(|l| scene.document_order(l.id, start.node).is_ge());
                    let last = leaves
                        .iter()
                        .rposition(|l| scene.document_order(l.id, end.node).is_le());
                    match (first, last) {
                        (Some(first), Some(last)) if first <= last => (first, last),
                        _ => (1, 0),
                    }
                }
            };
            for leaf in leaves.get(first..=last).unwrap_or_default() {
                let Some(text) = text_of(&scene, leaf.id) else {
                    continue;
                };
                let from = if leaf.id == start.node {
                    boundary(&text, start.offset)
                } else {
                    0
                };
                let to = if leaf.id == end.node {
                    boundary(&text, end.offset)
                } else {
                    text.len()
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
        let old = std::mem::replace(&mut state.shown, shown.clone());
        let color = colors.map(|(active, inactive)| match state.inactive {
            true => inactive,
            false => active,
        });
        {
            let scene = self.scene.borrow();
            state.synced = (
                scene.content_revision(),
                scene.layout_revision(),
                scene.text_shaper_revision(),
            );
        }
        drop(state);
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
        let Some(color) = color else {
            return;
        };
        for part in &shown {
            if !recolor && before.get(&part.node) == Some(&part) {
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
                .set_text_highlight(part.node, Some(TextHighlight { color, rects }));
        }
        let gesture = self.selection_state().borrow().gesture.is_some();
        if !gesture {
            self.selection_state().borrow_mut().layouts.clear();
        }
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
                return;
            }
        }
        self.refresh_selection(false);
    }
    /// The selection's anchor and focus, if there is one (it may be
    /// collapsed: both at one place).
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
    /// Select all the text in the selectable region holding `node`.
    pub fn select_all_text(&self, node: NodeId) -> bool {
        let (root, first, last) = {
            let scene = self.scene.borrow();
            let Some(root) = scene.selection_root(node) else {
                return false;
            };
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
                root,
                first.map(|node| TextPoint { node, offset: 0 }),
                last.map(|node| TextPoint { node, offset: end }),
            )
        };
        self.selection_state().borrow_mut().active_root = Some(root);
        if first.is_none() {
            return false;
        }
        self.set_selection(Some(root), first, last);
        true
    }
    pub fn clear_text_selection(&self) {
        self.set_selection(None, None, None);
    }
    /// The selected text as plain text: a node's selected part as it is
    /// (soft wraps join, its own line breaks stay), and between nodes what
    /// their layout shows (see `separator`). Text that does not show is
    /// left out.
    pub fn selected_text(&self) -> String {
        let shown = self.selection_state().borrow().shown.clone();
        let scene = self.scene.borrow();
        let mut out = String::new();
        let mut previous: Option<(NodeId, f32)> = None;
        for part in shown {
            if !scene.layout_visible(part.node) {
                continue;
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
    /// What the selection makes of an event the document has dispatched:
    /// select-all and the end of a selection on a press elsewhere.
    pub(crate) fn selection_after_dispatch(
        &self,
        event: SelectionEvent,
        result: &mut crate::input::DispatchResult,
    ) {
        match event {
            SelectionEvent::None => {}
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

/// What `Ui`'s dispatch looks for after the document has had an event.
#[derive(Clone, Copy)]
pub(crate) enum SelectionEvent {
    None,
    SelectAll,
    Press,
}
impl SelectionEvent {
    pub(crate) fn of(event: &InputEvent) -> Self {
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
            } => Self::Press,
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

/// `offset` within `text`, on a character boundary at or before it.
fn boundary(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn order(scene: &Scene, a: TextPoint, b: TextPoint) -> std::cmp::Ordering {
    scene
        .document_order(a.node, b.node)
        .then(a.offset.cmp(&b.offset))
}
