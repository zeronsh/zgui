//! Where text can be selected (CSS `user-select`) and how a selection shows
//! on it. Both are rare, kept beside the nodes: a scene without selectable
//! text pays a length check for them.
use super::{
    Color, NodeId, NodeKind, PAINTED, Rect, Scene, Transform, clipped_bounds, own_visible,
};
use std::collections::HashMap;

/// A text selection's highlight on one text node: rectangles from its box's
/// origin, painted under its glyphs and within its box, with their corner
/// `radius`; with `text_color`, the glyphs whose centres they hold paint in
/// it (CSS `::selection { color }`), from the glyphs already laid out.
#[derive(Clone, Debug, PartialEq)]
pub struct TextHighlight {
    /// The highlight's fill colour.
    pub color: Color,
    /// The highlighted rectangles, from the text box's origin.
    pub rects: Vec<Rect>,
    /// The colour the glyphs they hold paint in; `None` keeps the glyphs' own.
    pub text_color: Option<Color>,
    /// The rectangles' corner radius.
    pub radius: f32,
}
impl TextHighlight {
    /// The colour a glyph centred at (`x`, `y`) in the node's box paints in,
    /// if the selection recolours it.
    pub fn glyph_color(&self, x: f32, y: f32) -> Option<Color> {
        self.text_color.filter(|_| {
            self.rects
                .iter()
                .any(|r| x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height)
        })
    }
}

/// A mark behind a text node's glyphs, apart from its selection: a find
/// match's (`Scene::set_text_marks`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextMark {
    /// Its rectangle, from the text box's origin.
    pub rect: Rect,
    /// Its fill.
    pub color: Color,
}

#[derive(Default)]
pub(super) struct Selectable {
    /// `Scene::set_user_select`'s settings.
    user_select: HashMap<NodeId, bool>,
    /// How many of them allow selection: none keeps cursors as they were.
    allowed: usize,
    highlights: HashMap<NodeId, TextHighlight>,
    marks: HashMap<NodeId, Vec<TextMark>>,
}
impl Selectable {
    /// Whether anything allows selection.
    pub(super) fn any(&self) -> bool {
        self.allowed > 0
    }
    /// `id` is gone.
    pub(super) fn forget(&mut self, id: NodeId) {
        if !self.user_select.is_empty() && self.user_select.remove(&id) == Some(true) {
            self.allowed -= 1;
        }
        if !self.highlights.is_empty() {
            self.highlights.remove(&id);
        }
        if !self.marks.is_empty() {
            self.marks.remove(&id);
        }
    }
}

/// A selectable text node `Scene::selectable_text` visits.
#[derive(Clone, Copy, Debug)]
pub struct TextLeaf {
    /// The text node.
    pub id: NodeId,
    /// Its box, translated (scrolling included).
    pub bounds: Rect,
    /// What of it shows within its ancestors' clips and the viewport.
    pub visible: Option<Rect>,
}

impl Scene {
    /// CSS `user-select`: `Some(true)` lets the pointer select the text in
    /// `id`'s subtree, `Some(false)` keeps it from doing so (`none`), and
    /// `None` takes what `id`'s parent has (the default: nothing is).
    pub fn set_user_select(&mut self, id: NodeId, select: Option<bool>) {
        if !self.contains(id) {
            return;
        }
        let state = &mut self.selectable;
        let old = match select {
            Some(value) => state.user_select.insert(id, value),
            None => state.user_select.remove(&id),
        };
        if old == select {
            return;
        }
        state.allowed =
            state.allowed + usize::from(select == Some(true)) - usize::from(old == Some(true));
        // The cursor over it changes.
        self.geometry_revision = self.geometry_revision.wrapping_add(1);
    }
    /// `id`'s own `user_select` setting.
    pub fn user_select(&self, id: NodeId) -> Option<bool> {
        if self.selectable.user_select.is_empty() {
            return None;
        }
        self.selectable.user_select.get(&id).copied()
    }
    /// Whether the pointer can select text in `id`: its own `user_select`
    /// or its nearest ancestor's that has one.
    pub fn is_selectable(&self, id: NodeId) -> bool {
        self.selection_root(id).is_some()
    }
    /// The node a selection made in `id` stays within (and select-all
    /// selects): the outermost of the ancestors, `id` included, that allow
    /// selection without one between them that forbids it. `None` if `id`
    /// is not selectable.
    pub fn selection_root(&self, id: NodeId) -> Option<NodeId> {
        if self.selectable.allowed == 0 || !self.contains(id) {
            return None;
        }
        let mut root = None;
        for node in std::iter::once(id).chain(self.ancestors(id)) {
            match self.selectable.user_select.get(&node) {
                Some(true) => root = Some(node),
                Some(false) => return root,
                None => {}
            }
        }
        root
    }
    /// Show (or with `None` stop showing) a selection on text node `id`.
    /// Repaints only where the highlight changed; nothing is laid out.
    pub fn set_text_highlight(&mut self, id: NodeId, highlight: Option<TextHighlight>) {
        if !self.contains(id) || self.selectable.highlights.get(&id) == highlight.as_ref() {
            return;
        }
        let old = match highlight {
            Some(highlight) => self.selectable.highlights.insert(id, highlight),
            None => self.selectable.highlights.remove(&id),
        };
        let new = self.selectable.highlights.get(&id);
        // A change of colour or shape repaints all of it; otherwise what was
        // or is highlighted, but not both.
        let look = |h: &TextHighlight| (h.color, h.text_color, h.radius.to_bits());
        let recolored = old.as_ref().map(look) != new.map(look);
        let changed: Vec<Rect> = match (&old, new) {
            (Some(old), Some(new)) if !recolored => old
                .rects
                .iter()
                .filter(|r| !new.rects.contains(r))
                .chain(new.rects.iter().filter(|r| !old.rects.contains(r)))
                .copied()
                .collect(),
            _ => old
                .iter()
                .chain(new)
                .flat_map(|h| h.rects.iter().copied())
                .collect(),
        };
        self.damage_text_rects(id, &changed);
        // Its layers repaint; its own retained glyphs stay as they are.
        self.invalidate(id, PAINTED);
    }
    /// The selection highlight shown on text node `id`, if any.
    pub fn text_highlight(&self, id: NodeId) -> Option<&TextHighlight> {
        if self.selectable.highlights.is_empty() {
            return None;
        }
        self.selectable.highlights.get(&id)
    }
    /// Show `marks` behind text node `id`'s glyphs, under its selection (none:
    /// no more). Repaints only where they changed; nothing is laid out.
    pub fn set_text_marks(&mut self, id: NodeId, marks: Vec<TextMark>) {
        let old = self
            .selectable
            .marks
            .get(&id)
            .map_or(&[][..], Vec::as_slice);
        if !self.contains(id) || old == marks.as_slice() {
            return;
        }
        let changed: Vec<Rect> = old
            .iter()
            .filter(|m| !marks.contains(m))
            .chain(marks.iter().filter(|m| !old.contains(m)))
            .map(|m| m.rect)
            .collect();
        if marks.is_empty() {
            self.selectable.marks.remove(&id);
        } else {
            self.selectable.marks.insert(id, marks);
        }
        self.damage_text_rects(id, &changed);
        self.invalidate(id, PAINTED);
    }
    /// The marks shown behind text node `id`'s glyphs.
    pub fn text_marks(&self, id: NodeId) -> &[TextMark] {
        if self.selectable.marks.is_empty() {
            return &[];
        }
        self.selectable.marks.get(&id).map_or(&[], Vec::as_slice)
    }
    /// The text nodes showing marks.
    pub fn marked_text(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.selectable.marks.keys().copied()
    }
    /// Whether any text shows a selection: renderers look no further if not.
    pub fn has_text_highlights(&self) -> bool {
        !self.selectable.highlights.is_empty()
    }
    /// What a highlight adds to a text node's ink.
    pub(super) fn highlight_ink(&self, id: NodeId, bounds: Rect, ink: Rect) -> Rect {
        let origin = Transform {
            x: bounds.x,
            y: bounds.y,
        };
        let ink = self
            .text_marks(id)
            .iter()
            .fold(ink, |ink, m| ink.union(m.rect.translated(origin)));
        match self.text_highlight(id) {
            Some(highlight) => highlight.rects.iter().fold(ink, |ink, r| {
                ink.union(r.translated(Transform {
                    x: bounds.x,
                    y: bounds.y,
                }))
            }),
            None => ink,
        }
    }
    fn damage_text_rects(&mut self, id: NodeId, rects: &[Rect]) {
        if rects.is_empty() || self.damage_is_full() || !self.shown(id) {
            return;
        }
        // Placed in a transformed or filtered layer: all of it.
        if self.transformed + self.filtered > 0 {
            self.damage_subtree(id);
            return;
        }
        let (bounds, effects, clip) = self.world(id);
        if effects.opacity <= 0. {
            return;
        }
        let clip = clip.unwrap_or(self.viewport);
        let origin = Transform {
            x: bounds.x,
            y: bounds.y,
        };
        for rect in rects {
            if let Some(rect) = rect.translated(origin).intersection(clip) {
                self.add_damage(rect);
            }
        }
    }
    /// The cursor selectable text shows under the pointer: the I-beam, over
    /// a text node in a selectable region that sets no cursor of its own
    /// between them. `None` leaves it to explicit cursors and roles.
    pub(super) fn selection_cursor(&self, hit: NodeId) -> Option<crate::cursor::Cursor> {
        if self.selectable.allowed == 0
            || !matches!(
                self.node(hit).kind,
                NodeKind::Text { .. } | NodeKind::RichText { .. }
            )
        {
            return None;
        }
        for node in std::iter::once(hit).chain(self.ancestors(hit)) {
            if self.node(node).cursor.is_some() {
                return None;
            }
            match self.selectable.user_select.get(&node) {
                Some(true) => return Some(crate::cursor::Cursor::Text),
                Some(false) => return None,
                None => {}
            }
        }
        None
    }
    /// Visit the selectable text under `root` in document order: displayed,
    /// not transparent, not in an inert or `user_select(false)` subtree, nor
    /// under a node `skip` excludes (an editor, which selects on its own).
    pub fn selectable_text(
        &self,
        root: NodeId,
        mut skip: impl FnMut(NodeId) -> bool,
        mut visit: impl FnMut(TextLeaf),
    ) {
        if !self.contains(root) {
            return;
        }
        let (origin, clip) = match self.node(root).parent {
            Some(parent) => {
                let (bounds, _, clip) = self.world(parent);
                let node = self.node(parent);
                (
                    Transform {
                        x: bounds.x - node.bounds.x,
                        y: bounds.y - node.bounds.y,
                    },
                    clipped_bounds(clip, bounds, &node.style),
                )
            }
            None => (Transform::default(), Some(self.viewport)),
        };
        let mut stack = vec![(root, origin, clip)];
        while let Some((id, parent, clip)) = stack.pop() {
            let node = self.node(id);
            if !own_visible(&node.style)
                || node.effects.opacity <= 0.
                || (id != root && self.selectable.user_select.get(&id) == Some(&false))
                || self.inert.contains(&id)
                || (id != root && skip(id))
            {
                continue;
            }
            let transform = Transform {
                x: parent.x + node.transform.x,
                y: parent.y + node.transform.y,
            };
            let bounds = node.bounds.translated(transform);
            if matches!(node.kind, NodeKind::Text { .. } | NodeKind::RichText { .. }) {
                visit(TextLeaf {
                    id,
                    bounds,
                    visible: clip.and_then(|clip| bounds.intersection(clip)),
                });
            }
            let inner = clipped_bounds(clip, bounds, &node.style);
            stack.extend(
                node.children
                    .iter()
                    .rev()
                    .map(|child| (*child, transform, inner)),
            );
        }
    }
    /// Window point (`x`, `y`) in the space `id` is laid out in (its
    /// `bounds`, translated): mapped through the inverse of each local
    /// transform (scale, rotation, skew) from the root down, `id`'s own
    /// included, as hit testing maps it. `None` where one is singular.
    /// Without transforms in the scene, the point itself.
    pub fn point_in(&self, id: NodeId, x: f32, y: f32) -> Option<(f32, f32)> {
        if self.transformed == 0 || !self.contains(id) {
            return Some((x, y));
        }
        let mut path: Vec<NodeId> = std::iter::once(id).chain(self.ancestors(id)).collect();
        path.reverse();
        let (mut translation, mut point) = (Transform::default(), (x, y));
        for node in path {
            let node = self.node(node);
            translation.x += node.transform.x;
            translation.y += node.transform.y;
            if node.transformed() {
                let bounds = node.bounds.translated(translation);
                point = node.projective(bounds).inverse()?.point(point.0, point.1);
            }
        }
        Some(point)
    }
    /// Order `a` and `b` as a document would: an ancestor before what it
    /// holds, siblings as their parent orders them. Both must be in the
    /// scene.
    pub fn document_order(&self, a: NodeId, b: NodeId) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        if a == b {
            return Ordering::Equal;
        }
        let path = |id: NodeId| {
            let mut path: Vec<NodeId> = std::iter::once(id).chain(self.ancestors(id)).collect();
            path.reverse();
            path
        };
        let (pa, pb) = (path(a), path(b));
        let shared = pa.iter().zip(&pb).take_while(|(x, y)| x == y).count();
        match (pa.get(shared), pb.get(shared)) {
            (None, _) => Ordering::Less,
            (_, None) => Ordering::Greater,
            (Some(x), Some(y)) => {
                let siblings = &self.node(pa[shared - 1]).children;
                let index = |id| siblings.iter().position(|c| c == id);
                index(x).cmp(&index(y))
            }
        }
    }
}
