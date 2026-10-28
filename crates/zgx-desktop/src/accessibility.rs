//! Native accessibility tree translation. Stable IDs survive incremental updates.
use accesskit::{Action, Node, NodeId as AccessibleId, Tree, TreeId, TreeUpdate};
use std::collections::{HashMap, HashSet};
use unicode_segmentation::UnicodeSegmentation;
use zgx::{
    scene::{NodeId, Scene},
    semantics::{Live, Role, ScrollAxis, SemanticNode, SemanticRevision, Semantics},
};

/// How often assistive technology hears of geometry alone changing (things
/// moving, scrolling, fading) while nothing it reads by name, value,
/// selection or focus did: an animation otherwise rebuilt the tree every
/// frame. AccessKit answers bounds and hit tests from the tree it was last
/// given, so those are at most this stale while content moves, and exact
/// once it stops.
pub(crate) const GEOMETRY_EVERY: std::time::Duration = std::time::Duration::from_millis(100);

/// Whether a change goes to assistive technology at once rather than paced
/// (`GeometryPacer`): focus moving always; names, values and selection
/// changing while a screen reader runs (`screen_reader`, asked only then),
/// as it speaks them. Other clients (the touch keyboard's host, voice
/// access) read names as they need them.
pub(crate) fn urgent(focused: bool, semantics: bool, screen_reader: impl FnOnce() -> bool) -> bool {
    focused || (semantics && screen_reader())
}

/// Whether a screen reader runs (Narrator, NVDA, JAWS set the system's flag,
/// `SPI_GETSCREENREADER`): what it speaks goes out at once. Elsewhere,
/// where no such flag is read, as if one always ran.
pub(crate) fn screen_reader() -> bool {
    #[cfg(windows)]
    {
        use windows::{
            Win32::UI::WindowsAndMessaging::{
                SPI_GETSCREENREADER, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
            },
            core::BOOL,
        };
        let mut on = BOOL(0);
        // SAFETY: reads one system setting into a local.
        let read = unsafe {
            SystemParametersInfoW(
                SPI_GETSCREENREADER,
                0,
                Some((&raw mut on).cast()),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            )
        };
        read.is_err() || on.as_bool()
    }
    #[cfg(not(windows))]
    {
        true
    }
}

/// When a window's accessibility tree is updated: at once for a change a
/// reader hears (`changed`: names, values, selection, focus, the first
/// tree), and for geometry alone at most every `GEOMETRY_EVERY`, a change
/// held meanwhile going out at its `deadline`.
#[derive(Default)]
pub(crate) struct GeometryPacer {
    last: Option<std::time::Instant>,
    held: Option<std::time::Instant>,
}
impl GeometryPacer {
    /// Whether to update the tree at `now`.
    pub(crate) fn update_now(
        &mut self,
        now: std::time::Instant,
        changed: bool,
        geometry: bool,
    ) -> bool {
        let due = self.last.is_none_or(|last| now >= last + GEOMETRY_EVERY);
        if changed || ((geometry || self.held.is_some()) && due) {
            self.last = Some(now);
            self.held = None;
            return true;
        }
        if geometry && let Some(last) = self.last {
            self.held.get_or_insert(last + GEOMETRY_EVERY);
        }
        false
    }
    /// When a held geometry change goes out, if one is held.
    pub(crate) fn deadline(&self) -> Option<std::time::Instant> {
        self.held
    }
}

/// Projects a scene's semantics into AccessKit tree updates, keeping each
/// node's accessible ID for as long as the node lives.
#[derive(Default)]
pub struct AccessibilityTree {
    ids: HashMap<NodeId, AccessibleId>,
    next: u64,
    cache: HashMap<AccessibleId, Node>,
    projection_revisions: HashMap<NodeId, SemanticRevision>,
    text_runs: HashMap<NodeId, Vec<AccessibleId>>,
    text_positions: HashMap<AccessibleId, (NodeId, Vec<usize>)>,
    /// Labels projected as blocks of a selectable region's text: their runs
    /// end with a break.
    blocks: HashSet<NodeId>,
    /// Long fields' runs, by line: the lines in their window (`long_lines`).
    long_runs: HashMap<NodeId, HashMap<usize, LineRuns>>,
    /// Where the text node showing a node's runs was when they were placed
    /// (`Scene::text_lines`): moved (scrolled in its field), they are again.
    run_boxes: HashMap<NodeId, zgx::scene::Rect>,
}
/// A line of a long field, as text runs: kept while its text is, so an edit
/// sends the runs of the lines it changed, and a scroll those it shows.
struct LineRuns {
    /// The line, with its break.
    text: String,
    /// Where it starts in the field's text, as its runs' positions say.
    start: usize,
    ids: Vec<AccessibleId>,
    /// Each run's characters' offsets, from the line's start.
    offsets: Vec<Vec<usize>>,
}
impl AccessibilityTree {
    /// An empty tree: the first update is a complete snapshot.
    pub fn new() -> Self {
        Self::default()
    }
    /// Force a complete snapshot when a native accessibility client reconnects.
    pub fn reset(&mut self) {
        self.cache.clear();
        self.projection_revisions.clear();
    }
    fn id(&mut self, node: NodeId) -> AccessibleId {
        *self.ids.entry(node).or_insert_with(|| {
            self.next += 1;
            AccessibleId(self.next)
        })
    }
    /// The scene node an accessible ID stands for, if it is still mounted.
    pub fn scene_node(&self, id: AccessibleId) -> Option<NodeId> {
        self.ids
            .iter()
            .find_map(|(node, a)| (*a == id).then_some(*node))
    }
    /// Convert native grapheme positions to committed UTF-8 byte offsets.
    /// Removed runs and selections spanning different editors are rejected.
    pub fn resolve_selection(
        &self,
        selection: &accesskit::TextSelection,
    ) -> Option<(NodeId, usize, usize)> {
        let (anchor_owner, anchor) = self.text_positions.get(&selection.anchor.node)?;
        let (focus_owner, focus) = self.text_positions.get(&selection.focus.node)?;
        if anchor_owner != focus_owner {
            return None;
        }
        Some((
            *anchor_owner,
            anchor[selection.anchor.character_index.min(anchor.len() - 1)],
            focus[selection.focus.character_index.min(focus.len() - 1)],
        ))
    }
    /// Convert a native selection across labels (a selectable region's) to
    /// each end's label and UTF-8 offset into its value.
    pub fn resolve_document_selection(
        &self,
        selection: &accesskit::TextSelection,
    ) -> Option<((NodeId, usize), (NodeId, usize))> {
        let end = |position: &accesskit::TextPosition| {
            let (owner, offsets) = self.text_positions.get(&position.node)?;
            Some((
                *owner,
                offsets[position.character_index.min(offsets.len() - 1)],
            ))
        };
        Some((end(&selection.anchor)?, end(&selection.focus)?))
    }
    /// A long field's runs, for `lines` of its `text`: a line's runs kept
    /// while its text is (moved, if it moved), made for the lines that
    /// changed or came into the window, and the rest let go.
    fn long_runs(
        &mut self,
        owner: NodeId,
        text: &zgx::text::Text,
        lines: std::ops::Range<usize>,
        nodes: &mut Vec<(AccessibleId, Node)>,
    ) {
        let mut was = self.long_runs.remove(&owner).unwrap_or_default();
        let mut kept = HashMap::with_capacity(lines.len());
        let mut ids = Vec::new();
        let count = text.line_count();
        for line in lines.start.min(count)..lines.end.min(count) {
            let range = text.line_range(line);
            let end = if line + 1 < count {
                range.end + 1
            } else {
                range.end
            };
            let value = text.slice(range.start..end);
            let mut entry = match was.remove(&line) {
                Some(entry)
                    if entry.text == *value
                        && entry.ids.iter().all(|id| self.cache.contains_key(id)) =>
                {
                    entry
                }
                old => {
                    // A grapheme too long for AccessKit: the line has no runs.
                    let Some(runs) = text_run_data(&value, false) else {
                        continue;
                    };
                    let mut run_ids = old.map(|old| old.ids).unwrap_or_default();
                    run_ids.truncate(runs.len());
                    while run_ids.len() < runs.len() {
                        self.next += 1;
                        run_ids.push(AccessibleId(self.next));
                    }
                    let count = runs.len();
                    let mut offsets = Vec::with_capacity(count);
                    for (index, run) in runs.into_iter().enumerate() {
                        offsets.push(run.offsets);
                        let mut node = Node::new(accesskit::Role::TextRun);
                        node.set_value(run.value);
                        node.set_character_lengths(run.lengths);
                        node.set_word_starts(run.words);
                        // A line's runs (cut at 255 characters) are one line.
                        if index > 0 {
                            node.set_previous_on_line(run_ids[index - 1]);
                        }
                        if index + 1 < count {
                            node.set_next_on_line(run_ids[index + 1]);
                        }
                        nodes.push((run_ids[index], node));
                    }
                    LineRuns {
                        text: value.into_owned(),
                        start: usize::MAX,
                        ids: run_ids,
                        offsets,
                    }
                }
            };
            if entry.start != range.start {
                entry.start = range.start;
                for (id, offsets) in entry.ids.iter().zip(&entry.offsets) {
                    let at = offsets.iter().map(|offset| offset + range.start).collect();
                    self.text_positions.insert(*id, (owner, at));
                }
            }
            ids.extend_from_slice(&entry.ids);
            kept.insert(line, entry);
        }
        self.long_runs.insert(owner, kept);
        if ids.is_empty() {
            self.text_runs.remove(&owner);
        } else {
            self.text_runs.insert(owner, ids);
        }
    }
    /// The native position of UTF-8 offset `byte` into label `owner`'s runs.
    fn text_position(&self, owner: NodeId, byte: usize) -> Option<accesskit::TextPosition> {
        let ids = self.text_runs.get(&owner)?;
        Some(run_position(ids, &self.text_positions, byte))
    }
    /// What changed in the accessible tree since the last update: nodes whose
    /// semantics changed, and the focus. `title` names the window and `scale`
    /// converts its logical bounds to physical pixels.
    pub fn update(
        &mut self,
        scene: &Scene,
        semantics: &Semantics,
        focused: Option<NodeId>,
        title: &str,
        scale: f64,
    ) -> TreeUpdate {
        let root = self.id(scene.root());
        let mut children: HashMap<NodeId, Vec<AccessibleId>> = HashMap::new();
        // Scene order, not hash-map iteration order, determines accessible traversal.
        let mut ordered: Vec<_> = scene
            .paint_items()
            .filter(|p| p.effects.opacity > 0.0)
            .filter_map(|p| semantics.get(p.id).map(|s| (p.id, s)))
            // Text with nothing in it (a count at zero) says nothing: it
            // joins the tree when it has something to say.
            .filter(|(_, s)| {
                s.role != Role::Label || !s.value.as_deref().unwrap_or(&s.label).is_empty()
            })
            .collect();
        // What is hidden from assistive technology (`aria-hidden`) goes with
        // its subtree; only a tree that hides something looks.
        let hidden: Vec<_> = ordered
            .iter()
            .filter(|(_, s)| s.hidden)
            .map(|(id, _)| *id)
            .collect();
        if !hidden.is_empty() {
            ordered.retain(|(id, _)| !hidden.iter().any(|h| scene.is_within(*id, *h)));
        }
        let visible: HashSet<_> = ordered
            .iter()
            .map(|(id, _)| *id)
            .chain(std::iter::once(scene.root()))
            .collect();
        self.ids.retain(|id, _| visible.contains(id));
        self.projection_revisions
            .retain(|id, _| visible.contains(id));
        let mut parents = HashMap::new();
        for (id, _) in &ordered {
            if *id == scene.root() {
                continue;
            }
            let mut parent = scene.parent(*id);
            while let Some(p) = parent {
                if visible.contains(&p) {
                    break;
                }
                parent = scene.parent(p);
            }
            parents.insert(*id, parent.unwrap_or(scene.root()));
        }
        // Apply logical ownership without changing physical clipping or bounds.
        // Each accepted edge keeps the graph acyclic; invalid overrides retain
        // their physical parent and never introduce duplicate native children.
        for (id, semantic) in &ordered {
            if *id == scene.root() {
                continue;
            }
            let Some(parent) = semantic.logical_parent.filter(|p| visible.contains(p)) else {
                continue;
            };
            let mut ancestor = parent;
            let valid = loop {
                if ancestor == *id {
                    break false;
                }
                if ancestor == scene.root() {
                    break true;
                }
                let Some(next) = parents.get(&ancestor) else {
                    break false;
                };
                ancestor = *next;
            };
            if valid {
                parents.insert(*id, parent);
            }
        }
        for (id, _) in &ordered {
            if *id == scene.root() {
                continue;
            }
            let child = self.id(*id);
            children.entry(parents[id]).or_default().push(child);
        }
        // Where each list's items are in it ("3 of 5", as a browser's
        // `<li>` reads), for items the app doesn't say it of: only a tree
        // with a list counts.
        let mut list_items: HashMap<NodeId, (usize, usize)> = HashMap::new();
        if ordered.iter().any(|(_, s)| s.role == Role::List) {
            let mut counts: HashMap<NodeId, usize> = HashMap::new();
            let mut lists: Vec<(NodeId, NodeId, usize)> = Vec::new();
            for (id, semantic) in &ordered {
                let Some(&list) = parents.get(id) else {
                    continue;
                };
                if semantic.role == Role::ListItem
                    && semantics.get(list).is_some_and(|s| s.role == Role::List)
                {
                    let count = counts.entry(list).or_default();
                    *count += 1;
                    lists.push((*id, list, *count));
                }
            }
            for (item, list, position) in lists {
                list_items.insert(item, (position, counts[&list]));
            }
        }
        self.text_runs.retain(|owner, _| visible.contains(owner));
        self.blocks.retain(|owner| visible.contains(owner));
        self.long_runs.retain(|owner, _| visible.contains(owner));
        // Fields saying what is wrong with them, and the text that shows it
        // (most trees have none: no node is looked up for one).
        let any_errors = semantics.has_errors();
        let error_shown = if any_errors {
            error_texts(scene, semantics, &ordered)
        } else {
            HashMap::new()
        };
        let mut nodes = Vec::with_capacity(ordered.len() + 1);
        // Selectable regions projected now, and their selections: set once
        // their labels' runs are.
        let mut documents = Vec::new();
        let mut window = Node::new(accesskit::Role::Window);
        window.set_label(title);
        window.set_children(children.remove(&scene.root()).unwrap_or_default());
        let bounds = scene.bounds(scene.root());
        window.set_bounds(accesskit::Rect::new(
            0.,
            0.,
            bounds.width as f64 * scale,
            bounds.height as f64 * scale,
        ));
        nodes.push((root, window));
        for (id, semantic) in ordered {
            if id == scene.root() {
                continue;
            }
            let native = self.id(id);
            let disabled = semantic.disabled
                || scene
                    .ancestors(id)
                    .any(|ancestor| semantics.get(ancestor).is_some_and(|s| s.disabled));
            // Assistive bounds: the box around where it shows.
            let r = scene.transformed_bounds(id);
            let revision = semantics
                .node_revision(id)
                .expect("projected semantic node");
            let bounds = accesskit::Rect::new(
                r.x as f64 * scale,
                r.y as f64 * scale,
                (r.x + r.width) as f64 * scale,
                (r.y + r.height) as f64 * scale,
            );
            let mut native_children = children.remove(&id).unwrap_or_default();
            let runs = self.text_runs.get(&id).map_or(&[][..], Vec::as_slice);
            // Text read as text: a label, or a text node that is a heading
            // (a label with a level). A heading holding text is named by
            // it, and its text reads as its own.
            let text = semantic.role == Role::Label
                || (semantic.role == Role::Heading
                    && matches!(
                        scene.kind(id),
                        zgx::scene::NodeKind::Text { .. } | zgx::scene::NodeKind::RichText { .. }
                    ));
            // A label in a selectable region is one of its blocks, which
            // break its text as a browser's paragraphs do.
            let block = text && scene.is_selectable(id);
            // Reuse the existing cached node's geometry/children instead of
            // retaining another copy in the revision cache. Text runs must still
            // have live cached nodes/positions before omitting this projection.
            let runs_at = semantics
                .text_node(id)
                .or(text.then_some(id))
                .filter(|node| scene.contains(*node))
                // Where it shows: a zoom or transform moves its runs too.
                .map(|node| scene.transformed_bounds(node));
            if self.projection_revisions.get(&id) == Some(&revision)
                && self.run_boxes.get(&id).copied() == runs_at.filter(|_| !runs.is_empty())
                && self.blocks.contains(&id) == block
                && self.cache.get(&native).is_some_and(|cached| {
                    let counted = list_items.get(&id);
                    cached.bounds() == Some(bounds)
                        && cached.is_disabled() == disabled
                        && cached.position_in_set()
                            == semantic.position_in_set.or(counted.map(|c| c.0))
                        && cached.size_of_set() == semantic.size_of_set.or(counted.map(|c| c.1))
                        && cached
                            .children()
                            .iter()
                            .eq(native_children.iter().chain(runs))
                })
                && runs.iter().all(|run| {
                    self.cache.contains_key(run) && self.text_positions.contains_key(run)
                })
            {
                continue;
            }
            let mut n = Node::new(semantic_role(semantic));
            // AccessKit Label content belongs in value; AT-SPI derives its
            // accessible name from value, rather than the control-label field.
            // A long field has none: its text is its runs, a page of it.
            let value = semantic.value.as_deref();
            let long = semantics.long_value(id).zip(semantics.long_lines(id));
            let text_value = if text {
                // A heading is also named by its text, as a browser's is.
                if semantic.role == Role::Heading {
                    n.set_label(semantic.label.clone());
                }
                Some(value.unwrap_or(&semantic.label))
            } else if semantic.role == Role::Heading {
                n.set_label(semantic.label.clone());
                None
            } else {
                n.set_label(semantic.label.clone());
                value
            };
            if let Some(active) = semantic.active_descendant.filter(|a| visible.contains(a)) {
                n.set_active_descendant(self.id(active));
            }
            if let Some(shortcut) = &semantic.shortcut {
                n.set_keyboard_shortcut(shortcut.clone());
            }
            if let Some(expanded) = semantic.expanded {
                n.set_expanded(expanded);
            }
            if let Some(popup) = semantic.has_popup {
                n.set_has_popup(match popup {
                    zgx::semantics::PopupKind::Menu => accesskit::HasPopup::Menu,
                    zgx::semantics::PopupKind::Dialog => accesskit::HasPopup::Dialog,
                });
            }
            if semantic.modal {
                n.set_modal();
            }
            if let Some(level) = semantic.level {
                n.set_level(level);
            }
            if let Some(selected) = semantic.selected {
                n.set_selected(selected);
            }
            // A status is politely live unless it says otherwise, as ARIA's is.
            match semantic
                .live
                .or((semantic.role == Role::Status).then_some(Live::Polite))
            {
                Some(Live::Polite) => n.set_live(accesskit::Live::Polite),
                Some(Live::Assertive) => n.set_live(accesskit::Live::Assertive),
                None => {}
            }
            // An invalid field's error: flagged, and pointing at the text
            // that shows it (`aria-errormessage`), which the repository's
            // adapters give the platform as the field's. Otherwise (no such
            // text, or crates.io's adapters, which read neither) it is said
            // after the description, so it is heard as the field is reached.
            let error = if any_errors {
                semantics.error(id)
            } else {
                None
            };
            let shown = error.and_then(|_| error_shown.get(&id).copied());
            if error.is_some() {
                n.set_invalid(accesskit::Invalid::True);
                if let Some(target) = shown {
                    let target = self.id(target);
                    n.set_error_message(target);
                    n.push_described_by(target);
                }
            }
            let error = error.filter(|_| !cfg!(zgx_accesskit_patched) || shown.is_none());
            match (&semantic.description, error) {
                (Some(description), Some(error)) => {
                    n.set_description(format!("{description}. {error}"))
                }
                (Some(description), None) => n.set_description(description.clone()),
                (None, Some(error)) => n.set_description(error.to_owned()),
                (None, None) => {}
            }

            // A heading's text is its name and its runs; its value is left
            // to the platform (its level, on macOS), or the document would
            // read it twice.
            let heading = semantic.role == Role::Heading;
            if let Some(v) = text_value.filter(|_| !heading) {
                n.set_value(v);
            }
            if let Some(placeholder) = &semantic.placeholder {
                n.set_placeholder(placeholder.clone());
            }
            if let Some(v) = semantic.checked {
                n.set_toggled(if semantic.mixed {
                    accesskit::Toggled::Mixed
                } else if v {
                    accesskit::Toggled::True
                } else {
                    accesskit::Toggled::False
                });
            }

            if disabled {
                n.set_disabled();
            }
            if semantic.read_only {
                n.set_read_only();
            }
            if let Some(v) = semantic.numeric_value {
                n.set_numeric_value(v);
            }
            if let Some(v) = semantic.min {
                n.set_min_numeric_value(v);
            }
            if let Some(v) = semantic.max {
                n.set_max_numeric_value(v);
            }
            if matches!(
                semantic.role,
                Role::Button
                    | Role::Link
                    | Role::CheckBox
                    | Role::MenuItem
                    | Role::ListBoxOption
                    | Role::Tab
                    | Role::RadioButton
                    | Role::Switch
                    | Role::TreeItem
                    | Role::ColumnHeader
            ) {
                n.add_action(Action::Click);
            }
            if matches!(
                semantic.role,
                Role::TextInput
                    | Role::MultilineTextInput
                    | Role::Slider
                    | Role::ScrollBar
                    | Role::SpinButton
                    | Role::ComboBox
                    | Role::Splitter
            ) && !semantic.read_only
            {
                n.add_action(Action::SetValue);
            }
            if matches!(
                semantic.role,
                Role::Button
                    | Role::Link
                    | Role::CheckBox
                    | Role::MenuItem
                    | Role::TextInput
                    | Role::MultilineTextInput
                    | Role::Slider
                    | Role::ScrollBar
                    | Role::Splitter
                    | Role::RadioButton
                    | Role::Switch
                    | Role::SpinButton
                    | Role::ComboBox
                    | Role::TreeItem
                    | Role::Cell
                    | Role::ColumnHeader
                    | Role::ListItem
                    | Role::ListBoxOption
                    | Role::Tab
                    | Role::Dialog
                    | Role::Menu
                    | Role::Tree
                    | Role::Grid
            ) {
                n.add_action(Action::Focus);
            }
            let counted = list_items.get(&id);
            if let Some(position) = semantic.position_in_set.or(counted.map(|c| c.0)) {
                n.set_position_in_set(position);
            }
            if let Some(size) = semantic.size_of_set.or(counted.map(|c| c.1)) {
                n.set_size_of_set(size);
            }
            if let Some(table) = &semantic.table {
                if let Some(row) = table.row {
                    n.set_row_index(row);
                }
                if let Some(column) = table.column {
                    n.set_column_index(column);
                }
                if let Some(rows) = table.rows {
                    n.set_row_count(rows);
                }
                if let Some(columns) = table.columns {
                    n.set_column_count(columns);
                }
                if table.multiselectable {
                    n.set_multiselectable();
                }
                match table.sort {
                    Some(zgx::semantics::SortOrder::Ascending) => {
                        n.set_sort_direction(accesskit::SortDirection::Ascending)
                    }
                    Some(zgx::semantics::SortOrder::Descending) => {
                        n.set_sort_direction(accesskit::SortDirection::Descending)
                    }
                    None => {}
                }
            }
            if semantic.role == Role::ScrollView && semantic.size_of_set.is_some() {
                n.add_action(Action::Focus);
            }
            if semantic.role == Role::ScrollView {
                match semantic.scroll_axis.unwrap_or(ScrollAxis::Vertical) {
                    ScrollAxis::Vertical => {
                        n.add_action(Action::ScrollUp);
                        n.add_action(Action::ScrollDown);
                    }
                    ScrollAxis::Horizontal => {
                        n.add_action(Action::ScrollLeft);
                        n.add_action(Action::ScrollRight);
                    }
                }
            }
            if matches!(
                semantic.role,
                Role::Slider | Role::ScrollBar | Role::Splitter
            ) {
                n.add_action(Action::Increment);
                n.add_action(Action::Decrement);
            }
            if semantic.role == Role::ScrollBar {
                n.set_orientation(match semantic.scroll_axis.unwrap_or(ScrollAxis::Vertical) {
                    ScrollAxis::Horizontal => accesskit::Orientation::Horizontal,
                    ScrollAxis::Vertical => accesskit::Orientation::Vertical,
                });
            }
            n.set_bounds(bounds);
            if text || matches!(semantic.role, Role::TextInput | Role::MultilineTextInput) {
                // The node its text shows in, while it shows that text (a
                // field composing shows other): its lines' geometry.
                let shown_in = semantics
                    .text_node(id)
                    .or(text.then_some(id))
                    .filter(|node| {
                        scene.contains(*node)
                            && match scene.kind(*node) {
                                zgx::scene::NodeKind::Text { text, .. } => {
                                    Some(&**text) == text_value
                                }
                                zgx::scene::NodeKind::RichText { .. } => text,
                                _ => false,
                            }
                    });
                let shown_box = shown_in.map(|node| scene.transformed_bounds(node));
                let was = self.run_boxes.get(&id).copied();
                let moved = was != shown_box;
                let unchanged = self
                    .ids
                    .get(&id)
                    .and_then(|native| self.cache.get(native))
                    .is_some_and(|cached| {
                        cached.role() == semantic_role(semantic)
                            && cached.value() == text_value.filter(|_| !heading)
                    })
                    && self.blocks.contains(&id) == block
                    && self.text_runs.get(&id).is_none_or(|ids| {
                        ids.iter().all(|run| {
                            self.cache.contains_key(run) && self.text_positions.contains_key(run)
                        })
                    });
                let reusable = !moved && unchanged;
                // Only moved, its box the same size (scrolled): its runs keep
                // their shape, their boxes going with it, nothing shaped again.
                let shifted = match (was, shown_box) {
                    (Some(was), Some(now))
                        if moved
                            && unchanged
                            && long.is_none()
                            && was.width == now.width
                            && was.height == now.height
                            && self.text_runs.contains_key(&id) =>
                    {
                        Some((
                            (now.x - was.x) as f64 * scale,
                            (now.y - was.y) as f64 * scale,
                            now,
                        ))
                    }
                    _ => None,
                };
                if let Some((dx, dy, now)) = shifted {
                    self.run_boxes.insert(id, now);
                    for run in &self.text_runs[&id] {
                        let Some(cached) = self.cache.get(run) else {
                            continue;
                        };
                        let mut moved_run = cached.clone();
                        if let Some(b) = moved_run.bounds() {
                            moved_run.set_bounds(accesskit::Rect::new(
                                b.x0 + dx,
                                b.y0 + dy,
                                b.x1 + dx,
                                b.y1 + dy,
                            ));
                        }
                        nodes.push((*run, moved_run));
                    }
                } else if let Some((long, lines)) = &long {
                    self.blocks.remove(&id);
                    self.long_runs(id, long, lines.clone(), &mut nodes);
                } else if !reusable {
                    if block {
                        self.blocks.insert(id);
                    } else {
                        self.blocks.remove(&id);
                    }
                    match shown_box {
                        Some(shown) => self.run_boxes.insert(id, shown),
                        None => self.run_boxes.remove(&id),
                    };
                    #[cfg(test)]
                    SHAPED.with(|shaped| shaped.set(shaped.get() + 1));
                    let lines = shown_in.and_then(|node| scene.text_lines(node));
                    if let Some(runs) =
                        text_run_data_on(text_value.unwrap_or(""), block, lines.as_deref(), scale)
                    {
                        let ids = self.text_runs.entry(id).or_default();
                        ids.truncate(runs.len());
                        while ids.len() < runs.len() {
                            self.next += 1;
                            ids.push(AccessibleId(self.next));
                        }
                        // Whether each run's line goes on into the next.
                        let joined: Vec<bool> = runs
                            .iter()
                            .map(|run| !run.wraps && !run.value.ends_with(['\n', '\r']))
                            .collect();
                        let count = runs.len();
                        for (index, run) in runs.into_iter().enumerate() {
                            let mut text = Node::new(accesskit::Role::TextRun);
                            text.set_value(run.value);
                            text.set_character_lengths(run.lengths);
                            text.set_word_starts(run.words);
                            if let Some(geometry) = run.geometry {
                                text.set_bounds(geometry.bounds);
                                text.set_character_positions(geometry.positions);
                                text.set_character_widths(geometry.widths);
                                text.set_text_direction(if geometry.rtl {
                                    accesskit::TextDirection::RightToLeft
                                } else {
                                    accesskit::TextDirection::LeftToRight
                                });
                            }
                            if index > 0 && joined[index - 1] {
                                text.set_previous_on_line(ids[index - 1]);
                            }
                            if index + 1 < count && joined[index] {
                                text.set_next_on_line(ids[index + 1]);
                            }
                            self.text_positions.insert(ids[index], (id, run.offsets));
                            nodes.push((ids[index], text));
                        }
                    } else {
                        self.text_runs.remove(&id);
                    }
                }
                if let Some(ids) = self.text_runs.get(&id) {
                    let selection_position =
                        |byte: usize| run_position(ids, &self.text_positions, byte);
                    if !text && let Some((anchor, focus)) = semantic.text_selection {
                        n.set_text_selection(accesskit::TextSelection {
                            anchor: selection_position(anchor),
                            focus: selection_position(focus),
                        });
                    }
                    if !text {
                        n.add_action(Action::SetTextSelection);
                    }
                    native_children.extend(ids.iter().copied());
                }
            } else {
                self.text_runs.remove(&id);
                self.blocks.remove(&id);
            }
            if let Some(selection) = semantic.document_selection {
                documents.push((nodes.len(), selection));
            }
            n.set_children(native_children);
            nodes.push((native, n));
            self.projection_revisions.insert(id, revision);
        }
        for (index, (anchor, focus)) in documents {
            if let (Some(anchor), Some(focus)) = (
                self.text_position(anchor.0, anchor.1),
                self.text_position(focus.0, focus.1),
            ) {
                let node = &mut nodes[index].1;
                node.set_text_selection(accesskit::TextSelection { anchor, focus });
                node.add_action(Action::SetTextSelection);
            }
        }
        let live: HashSet<_> = self
            .ids
            .values()
            .copied()
            .chain(self.text_runs.values().flatten().copied())
            .collect();
        self.text_positions.retain(|run, _| live.contains(run));
        self.cache.retain(|id, _| live.contains(id));
        nodes.retain(|(id, node)| {
            if self.cache.get(id) == Some(node) {
                false
            } else {
                self.cache.insert(*id, node.clone());
                true
            }
        });
        let mut tree = Tree::new(root);
        tree.toolkit_name = Some("zgx".into());
        tree.toolkit_version = Some(env!("CARGO_PKG_VERSION").into());
        TreeUpdate {
            nodes,
            tree: Some(tree),
            tree_id: TreeId::ROOT,
            focus: focused
                .and_then(|n| self.ids.get(&n).copied())
                .unwrap_or(root),
        }
    }
}
// AccessKit character lengths and word indices are u8. Bound each run to
// 255 selectable graphemes, preserving line linkage across split runs. A single
// grapheme exceeding 255 UTF-8 bytes has no faithful representation in this
// AccessKit version; keep the editor value but omit its Text interface.
struct TextRunData {
    value: String,
    lengths: Vec<u8>,
    offsets: Vec<usize>,
    words: Vec<u8>,
    /// Where it shows (`text_run_data_on`): its box, and each character's
    /// place and width across it, in its direction; in physical pixels.
    geometry: Option<RunGeometry>,
    /// It ends where its line wraps: the next run is on the next line.
    wraps: bool,
}
#[cfg(test)]
thread_local! {
    /// Texts whose runs were placed from their shaping: none for a text
    /// that only moved.
    static SHAPED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
struct RunGeometry {
    bounds: accesskit::Rect,
    positions: Vec<f32>,
    widths: Vec<f32>,
    rtl: bool,
}
/// The native position of UTF-8 offset `byte` into the label whose runs are
/// `ids`: before the character starting there, so a block's end is before
/// its break.
fn run_position(
    ids: &[AccessibleId],
    positions: &HashMap<AccessibleId, (NodeId, Vec<usize>)>,
    byte: usize,
) -> accesskit::TextPosition {
    let index = ids
        .iter()
        .rposition(|run| positions[run].1[0] <= byte)
        .unwrap_or(0);
    let offsets = &positions[&ids[index]].1;
    let after = offsets.partition_point(|offset| *offset < byte);
    accesskit::TextPosition {
        node: ids[index],
        character_index: if offsets.get(after) == Some(&byte) {
            after
        } else {
            after.saturating_sub(1)
        },
    }
}
/// `value`'s runs; a `block`'s end with a line break (zero-width in
/// `value`, at its end) unless its text already does.
fn text_run_data(value: &str, block: bool) -> Option<Vec<TextRunData>> {
    text_run_data_on(value, block, None, 1.)
}
/// `text_run_data`, cut also where `lines` (how `value` shows,
/// `Scene::text_lines`) wrap, each run given its box and its characters'
/// places at `scale`: what lets a screen reader's cursor and a magnifier
/// follow the caret and the selection.
fn text_run_data_on(
    value: &str,
    block: bool,
    lines: Option<&[zgx::scene::TextLine]>,
    scale: f64,
) -> Option<Vec<TextRunData>> {
    if let Some(lines) = lines {
        for line in lines {
            value.get(line.range.clone())?;
        }
    }
    // Lines follow the text's order, as its graphemes do: one walk gives
    // each its line and edges.
    let mut walk = lines.map(|lines| LineWalk {
        lines,
        index: 0,
        grapheme: 0,
    });
    let mut words = value
        .unicode_word_indices()
        .map(|(offset, _)| offset)
        .peekable();
    let finish = |mut run: TextRunData, chars: &[Option<Edge>], wraps: bool| {
        run.geometry = lines.and_then(|lines| place(lines, chars, scale));
        run.wraps = wraps;
        run
    };
    let fresh = |offset: usize| TextRunData {
        value: String::new(),
        lengths: Vec::new(),
        offsets: vec![offset],
        words: Vec::new(),
        geometry: None,
        wraps: false,
    };
    let mut runs = Vec::new();
    let mut run = fresh(0);
    let mut chars: Vec<Option<Edge>> = Vec::new();
    for (offset, grapheme) in value.grapheme_indices(true) {
        let length = u8::try_from(grapheme.len()).ok()?;
        let (edge, wrap_start) = match &mut walk {
            Some(walk) => walk.edge(value, offset)?,
            None => (None, false),
        };
        let wrap = !run.lengths.is_empty() && wrap_start;
        if run.lengths.len() == 255 || wrap {
            runs.push(finish(
                std::mem::replace(&mut run, fresh(offset)),
                &chars,
                wrap,
            ));
            chars.clear();
        }
        while words.next_if(|start| *start < offset).is_some() {}
        if words.next_if_eq(&offset).is_some() {
            run.words.push(run.lengths.len() as u8);
        }
        run.value.push_str(grapheme);
        run.lengths.push(length);
        run.offsets.push(offset + grapheme.len());
        chars.push(edge);
        if grapheme.ends_with(['\n', '\r']) {
            let next = fresh(offset + grapheme.len());
            runs.push(finish(std::mem::replace(&mut run, next), &chars, false));
            chars.clear();
        }
    }
    // The block's break, zero-wide at the end of the last line.
    let end = match &mut walk {
        Some(walk) => walk.edge(value, value.len())?.0,
        None => None,
    };
    if block && !value.is_empty() && !value.ends_with(['\n', '\r']) {
        if run.lengths.len() == 255 {
            runs.push(finish(
                std::mem::replace(&mut run, fresh(value.len())),
                &chars,
                false,
            ));
            chars.clear();
        }
        run.value.push('\n');
        run.lengths.push(1);
        run.offsets.push(value.len());
        chars.push(end);
    }
    runs.push(finish(run, &chars, false));
    Some(runs)
}
/// A grapheme's line and its left and right edges.
type Edge = (usize, f32, f32);
/// Where `text_run_data_on`'s walk is in a text's lines.
struct LineWalk<'a> {
    lines: &'a [zgx::scene::TextLine],
    /// The first line not ended before the grapheme walked to.
    index: usize,
    /// How many of its graphemes were walked.
    grapheme: usize,
}
impl LineWalk<'_> {
    /// The grapheme at `offset` (each in turn): its edges, if it shows
    /// (a break, in no line, sits zero-wide at the end of the line before
    /// it), and whether it starts a line that wraps. None if its line has
    /// no carets for it.
    fn edge(&mut self, value: &str, offset: usize) -> Option<(Option<Edge>, bool)> {
        while let Some(line) = self.lines.get(self.index)
            && line.range.end <= offset
        {
            self.index += 1;
            self.grapheme = 0;
        }
        match self.lines.get(self.index) {
            Some(line) if line.range.start <= offset => {
                let wraps = self.grapheme == 0
                    && self.index > 0
                    && offset == line.range.start
                    && !value[..offset].ends_with(['\n', '\r']);
                let i = self.grapheme;
                self.grapheme += 1;
                let edge = (self.index, *line.carets.get(i)?, *line.carets.get(i + 1)?);
                Some((Some(edge), wraps))
            }
            _ => {
                let before = self.index.checked_sub(1).and_then(|index| {
                    let x = *self.lines[index].carets.last()?;
                    Some((index, x, x))
                });
                Some((before, false))
            }
        }
    }
}
/// A run's box and its characters' places, from their edges (`chars`).
fn place(
    lines: &[zgx::scene::TextLine],
    chars: &[Option<Edge>],
    scale: f64,
) -> Option<RunGeometry> {
    let line = &lines[(*chars.first()?)?.0];
    let rtl = line.rtl;
    let mut left = f32::MAX;
    let mut right = f32::MIN;
    for edge in chars {
        let (_, a, b) = (*edge)?;
        left = left.min(a.min(b));
        right = right.max(a.max(b));
    }
    let scale_f = scale as f32;
    let mut positions = Vec::with_capacity(chars.len());
    let mut widths = Vec::with_capacity(chars.len());
    for (_, a, b) in chars.iter().flatten() {
        let start = if rtl {
            right - a.max(*b)
        } else {
            a.min(*b) - left
        };
        positions.push(start * scale_f);
        widths.push((b - a).abs() * scale_f);
    }
    Some(RunGeometry {
        bounds: accesskit::Rect::new(
            left as f64 * scale,
            line.rect.y as f64 * scale,
            right as f64 * scale,
            (line.rect.y + line.rect.height) as f64 * scale,
        ),
        positions,
        widths,
        rtl,
    })
}

fn semantic_role(semantic: &zgx::semantics::SemanticNode) -> accesskit::Role {
    if semantic.role == Role::MenuItem && semantic.checked.is_some() && semantic.radio {
        accesskit::Role::MenuItemRadio
    } else if semantic.role == Role::MenuItem && semantic.checked.is_some() {
        accesskit::Role::MenuItemCheckBox
    } else {
        role(semantic.role)
    }
}
fn role(r: Role) -> accesskit::Role {
    match r {
        Role::Window => accesskit::Role::Window,
        Role::Group => accesskit::Role::GenericContainer,
        Role::Label => accesskit::Role::Label,
        Role::Button => accesskit::Role::Button,
        Role::Link => accesskit::Role::Link,
        Role::CheckBox => accesskit::Role::CheckBox,
        Role::Slider => accesskit::Role::Slider,
        Role::TextInput => accesskit::Role::TextInput,
        Role::MultilineTextInput => accesskit::Role::MultilineTextInput,
        Role::Progress => accesskit::Role::ProgressIndicator,
        Role::ScrollView => accesskit::Role::ScrollView,
        Role::ScrollBar => accesskit::Role::ScrollBar,
        Role::Splitter => accesskit::Role::Splitter,
        Role::ListItem => accesskit::Role::ListItem,
        Role::Dialog => accesskit::Role::Dialog,
        Role::Menu => accesskit::Role::Menu,
        Role::MenuItem => accesskit::Role::MenuItem,
        Role::Image => accesskit::Role::Image,
        Role::Document => accesskit::Role::Document,
        Role::Article => accesskit::Role::Article,
        Role::Heading => accesskit::Role::Heading,
        Role::List => accesskit::Role::List,
        Role::ListBox => accesskit::Role::ListBox,
        Role::ListBoxOption => accesskit::Role::ListBoxOption,
        Role::Toolbar => accesskit::Role::Toolbar,
        Role::Status => accesskit::Role::Status,
        Role::TabList => accesskit::Role::TabList,
        Role::Tab => accesskit::Role::Tab,
        Role::TabPanel => accesskit::Role::TabPanel,
        Role::RadioGroup => accesskit::Role::RadioGroup,
        Role::RadioButton => accesskit::Role::RadioButton,
        Role::Switch => accesskit::Role::Switch,
        Role::SpinButton => accesskit::Role::SpinButton,
        Role::ComboBox => accesskit::Role::ComboBox,
        Role::Tree => accesskit::Role::Tree,
        Role::TreeItem => accesskit::Role::TreeItem,
        Role::Grid => accesskit::Role::Grid,
        Role::Row => accesskit::Role::Row,
        Role::Cell => accesskit::Role::Cell,
        Role::ColumnHeader => accesskit::Role::ColumnHeader,
        Role::Alert => accesskit::Role::Alert,
    }
}

/// Translate native page-scroll actions through the same routing as wheel input.
pub(crate) fn scroll_action(ui: &zgx::widgets::Ui, node: NodeId, action: Action) -> bool {
    let (axis, direction) = match action {
        Action::ScrollUp => (ScrollAxis::Vertical, -1.),
        Action::ScrollDown => (ScrollAxis::Vertical, 1.),
        Action::ScrollLeft => (ScrollAxis::Horizontal, -1.),
        Action::ScrollRight => (ScrollAxis::Horizontal, 1.),
        _ => return false,
    };
    if !ui.scene.borrow().contains(node)
        || !ui.semantics.borrow().get(node).is_some_and(|n| {
            n.role == Role::ScrollView && n.scroll_axis.unwrap_or(ScrollAxis::Vertical) == axis
        })
    {
        return false;
    }
    // A page is the content box: inside the padding and border.
    let (bounds, padding) = {
        let scene = ui.scene.borrow();
        (scene.bounds(node), scene.content_insets(node))
    };
    let (delta_x, delta_y) = match axis {
        ScrollAxis::Horizontal => (
            direction * (bounds.width - padding.left - padding.right).max(0.),
            0.,
        ),
        ScrollAxis::Vertical => (
            0.,
            direction * (bounds.height - padding.top - padding.bottom).max(0.),
        ),
    };
    ui.input.dispatch_to(
        &ui.scene,
        node,
        zgx::input::InputEvent::Scroll {
            x: bounds.x + bounds.width / 2.,
            y: bounds.y + bounds.height / 2.,
            delta_x,
            delta_y,
        },
    );
    true
}

/// Each invalid field's error text as shown: the first label after it, in
/// the nearest of its three closest ancestors holding one, that says what
/// its error says.
fn error_texts(
    scene: &Scene,
    semantics: &Semantics,
    ordered: &[(NodeId, &SemanticNode)],
) -> HashMap<NodeId, NodeId> {
    let mut out = HashMap::new();
    for (i, (field, _)) in ordered.iter().enumerate() {
        let Some(error) = semantics.error(*field) else {
            continue;
        };
        let after = &ordered[i + 1..];
        let found = scene.ancestors(*field).take(3).find_map(|holder| {
            after.iter().find_map(|(id, s)| {
                (s.role == Role::Label
                    && s.value.as_deref().unwrap_or(&s.label) == error
                    && scene.is_within(*id, holder))
                .then_some(*id)
            })
        });
        if let Some(text) = found {
            out.insert(*field, text);
        }
    }
    out
}

#[cfg(test)]
mod scroll_tests {
    use super::*;
    use zgx::{compose::prelude::*, widgets::Ui};
    #[test]
    fn horizontal_native_actions_use_width_and_reject_vertical_requests() {
        let mut ui = Ui::new(200., 200.);
        let offset = ui.signal(0.);
        let view = ui.mount(
            scroll_x(offset.clone())
                .size(100., 80.)
                .p(8.)
                .child(div().w(300.)),
        );
        ui.prepare_frame();
        let node = view.node();
        assert!(!scroll_action(&ui, node, Action::ScrollDown));
        assert_eq!(offset.get(), 0.);
        assert!(scroll_action(&ui, node, Action::ScrollRight));
        assert_eq!(offset.get(), 84.);
        scroll_action(&ui, node, Action::ScrollRight);
        assert_eq!(offset.get(), 168.);
        scroll_action(&ui, node, Action::ScrollRight);
        assert_eq!(offset.get(), 216.);
        scroll_action(&ui, node, Action::ScrollLeft);
        assert_eq!(offset.get(), 132.);
        ui.set_disabled(node, true);
        scroll_action(&ui, node, Action::ScrollLeft);
        assert_eq!(offset.get(), 132.);
    }
    #[test]
    fn native_scroll_actions_page_clamp_and_respect_disabled() {
        let mut ui = Ui::new(200., 200.);
        let offset = ui.signal(0.);
        let view = ui.mount(
            scroll(offset.clone())
                .size(100., 80.)
                .p(8.)
                .child(div().h(200.)),
        );
        ui.prepare_frame();
        let node = view.node();
        assert!(scroll_action(&ui, node, Action::ScrollDown));
        assert_eq!(offset.get(), 64.);
        scroll_action(&ui, node, Action::ScrollDown);
        assert_eq!(offset.get(), 128.);
        scroll_action(&ui, node, Action::ScrollDown);
        assert_eq!(offset.get(), 136.);
        scroll_action(&ui, node, Action::ScrollUp);
        assert_eq!(offset.get(), 72.);
        ui.set_disabled(node, true);
        scroll_action(&ui, node, Action::ScrollUp);
        assert_eq!(offset.get(), 72.);
        view.unmount();
        assert!(!scroll_action(&ui, node, Action::ScrollDown));
    }
}

#[cfg(test)]
mod editor_text_tests {
    use super::*;
    use zgx::{compose::prelude::*, widgets::Ui};

    #[test]
    fn read_only_projection_preserves_text_selection_and_focus_actions() {
        let mut ui = Ui::new(400., 200.);
        let value = ui.signal(String::from("Read me"));
        let view = ui.mount(text_input("Document", value).size(300., 40.));
        ui.input.focus(&ui.scene, Some(view.node()));
        let editor = ui.focused_editor().unwrap();
        let mut adapter = AccessibilityTree::new();
        for read_only in [false, true, false] {
            editor.set_read_only(read_only);
            ui.prepare_frame();
            let update = adapter.update(
                &ui.scene.borrow(),
                &ui.semantics.borrow(),
                Some(view.node()),
                "Test",
                1.,
            );
            let node = &update
                .nodes
                .iter()
                .find(|(id, _)| *id == update.focus)
                .unwrap()
                .1;
            assert_eq!(node.is_read_only(), read_only);
            assert_eq!(node.supports_action(Action::SetValue), !read_only);
            assert!(node.supports_action(Action::SetTextSelection));
            assert!(node.supports_action(Action::Focus));
            assert!(!node.is_disabled());
        }
    }

    /// A zoomed field's runs are where it shows them: twice as large,
    /// through its zoom, as a magnifier following the caret needs.
    #[test]
    fn a_zoomed_fields_text_runs_are_where_it_shows_them() {
        let run = |zoom: f32| {
            let mut ui = Ui::new(600., 400.);
            let value = ui.signal("one two".to_owned());
            let view = ui.mount(
                column()
                    .zoom(zoom)
                    .child(text_area("Notes", value).size(200., 100.)),
            );
            ui.prepare_frame();
            let mut adapter = AccessibilityTree::new();
            let update = adapter.update(
                &ui.scene.borrow(),
                &ui.semantics.borrow(),
                Some(view.node()),
                "Test",
                1.,
            );
            update
                .nodes
                .into_iter()
                .find(|(_, node)| node.role() == accesskit::Role::TextRun)
                .and_then(|(_, node)| node.bounds())
                .expect("a run")
        };
        let (plain, zoomed) = (run(1.), run(2.));
        assert!(
            (zoomed.width() - plain.width() * 2.).abs() < 0.5,
            "{plain:?} {zoomed:?}"
        );
        assert!(
            (zoomed.height() - plain.height() * 2.).abs() < 0.5,
            "{plain:?} {zoomed:?}"
        );
    }

    #[test]
    fn a_fields_text_runs_say_where_each_character_shows() {
        // What a screen reader's cursor and a magnifier follow: each run's
        // box and its characters' places, so the selection's box (macOS's
        // AXBoundsForRange, AT-SPI's range extents) lands on its text.
        let mut ui = Ui::new(400., 300.);
        let value = ui.signal("one two\nthree four".to_owned());
        let view = ui.mount(text_area("Notes", value.clone()).size(300., 200.));
        ui.prepare_frame();
        ui.input.focus(&ui.scene, Some(view.node()));
        let editor = ui.focused_editor().unwrap();
        editor.editor.borrow_mut().set_selection(4, 7);
        editor.refresh();
        ui.prepare_frame();
        let scale = 2.;
        let mut adapter = AccessibilityTree::new();
        let update = adapter.update(
            &ui.scene.borrow(),
            &ui.semantics.borrow(),
            Some(view.node()),
            "Test",
            scale,
        );
        let runs: Vec<_> = update
            .nodes
            .iter()
            .filter(|(_, node)| node.role() == accesskit::Role::TextRun)
            .collect();
        assert_eq!(runs.len(), 2, "a run a line");
        for (_, run) in &runs {
            let positions = run.character_positions().expect("places");
            assert_eq!(positions.len(), run.character_lengths().len());
            assert_eq!(run.character_widths().unwrap().len(), positions.len());
            assert!(positions.windows(2).all(|w| w[1] >= w[0]), "{positions:?}");
            assert!(run.bounds().is_some());
        }
        let tree = accesskit_consumer::Tree::new(update, true);
        let focus = tree.state().focus().unwrap();
        let boxes = focus.text_selection().unwrap().bounding_boxes();
        assert_eq!(boxes.len(), 1, "{boxes:?}");
        // Where "two" shows, at twice the points.
        let text_node = ui.semantics.borrow().text_node(view.node()).unwrap();
        let lines = ui.scene.borrow().text_lines(text_node).unwrap();
        let (left, right) = (lines[0].carets[4] as f64, lines[0].carets[7] as f64);
        let shown = boxes[0];
        assert!(
            (shown.x0 - left * scale).abs() < 0.5,
            "{shown:?} from {left}"
        );
        assert!(
            (shown.x1 - right * scale).abs() < 0.5,
            "{shown:?} to {right}"
        );
        assert!((shown.y0 - lines[0].rect.y as f64 * scale).abs() < 0.5);
        // A wrapped paragraph's rows are runs of their own, not linked as
        // one line.
        let mut ui = Ui::new(400., 300.);
        let view = ui.mount(
            text_area("Notes", ui.signal("one two three four five".to_owned()))
                .size(80., 200.)
                .text_wrap(true),
        );
        ui.prepare_frame();
        ui.input.focus(&ui.scene, Some(view.node()));
        ui.prepare_frame();
        let mut adapter = AccessibilityTree::new();
        let update = adapter.update(
            &ui.scene.borrow(),
            &ui.semantics.borrow(),
            Some(view.node()),
            "Test",
            scale,
        );
        let runs: Vec<_> = update
            .nodes
            .iter()
            .filter(|(_, node)| node.role() == accesskit::Role::TextRun)
            .map(|(_, node)| node)
            .collect();
        assert!(runs.len() > 1, "{} runs once wrapped", runs.len());
        assert!(runs.iter().all(|run| run.next_on_line().is_none()));
        let tops: Vec<f64> = runs.iter().map(|run| run.bounds().unwrap().y0).collect();
        assert!(
            tops.windows(2).all(|w| w[1] > w[0]),
            "a row below the last: {tops:?}"
        );
    }

    #[test]
    fn native_text_reads_unicode_lines_and_maps_selection_without_byte_truncation() {
        struct Changes;
        impl accesskit_consumer::TreeChangeHandler for Changes {
            fn node_added(&mut self, _: &accesskit_consumer::Node) {}
            fn node_updated(&mut self, _: &accesskit_consumer::Node, _: &accesskit_consumer::Node) {
            }
            fn focus_moved(
                &mut self,
                _: Option<&accesskit_consumer::Node>,
                _: Option<&accesskit_consumer::Node>,
            ) {
            }
            fn node_removed(&mut self, _: &accesskit_consumer::Node) {}
        }
        let mut ui = Ui::new(400., 300.);
        let value = ui.signal("a👩‍💻e\u{301}\n你好\n".to_owned());
        let view = ui.mount(text_area("Document", value.clone()).size(300., 200.));
        ui.prepare_frame();
        ui.input.focus(&ui.scene, Some(view.node()));
        let editor = ui.focused_editor().unwrap();
        editor
            .editor
            .borrow_mut()
            .set_selection(1, value.get().len());
        editor.refresh();
        ui.prepare_frame();
        let mut adapter = AccessibilityTree::new();
        let update = adapter.update(
            &ui.scene.borrow(),
            &ui.semantics.borrow(),
            Some(view.node()),
            "Test",
            1.,
        );
        let selection = *update
            .nodes
            .iter()
            .find(|(id, _)| *id == update.focus)
            .unwrap()
            .1
            .text_selection()
            .unwrap();
        assert_eq!(
            adapter.resolve_selection(&selection),
            Some((view.node(), 1, value.get().len()))
        );
        let mut tree = accesskit_consumer::Tree::new(update, true);
        let node = tree.state().focus().unwrap();
        assert!(node.supports_text_ranges());
        assert_eq!(node.document_range().text(), value.get());
        assert_eq!(node.text_selection().unwrap().text(), value.get()[1..]);
        let run_ids = adapter.text_runs[&view.node()].clone();
        editor.editor.borrow_mut().set_selection(0, 1);
        editor.refresh();
        ui.prepare_frame();
        let moved = adapter.update(
            &ui.scene.borrow(),
            &ui.semantics.borrow(),
            Some(view.node()),
            "Test",
            1.,
        );
        assert_eq!(
            moved.nodes.len(),
            1,
            "selection-only updates emit only the editor"
        );
        assert_eq!(moved.nodes[0].0, moved.focus);
        assert_eq!(adapter.text_runs[&view.node()], run_ids);
        tree.update_and_process_changes(moved, &mut Changes);
        assert_eq!(
            tree.state().focus().unwrap().document_range().text(),
            value.get()
        );
        assert_eq!(
            tree.state()
                .focus()
                .unwrap()
                .text_selection()
                .unwrap()
                .text(),
            "a"
        );
        for read_only in [true, false] {
            editor.set_read_only(read_only);
            ui.prepare_frame();
            let changed = adapter.update(
                &ui.scene.borrow(),
                &ui.semantics.borrow(),
                Some(view.node()),
                "Test",
                1.,
            );
            assert_eq!(
                changed.nodes.len(),
                1,
                "read-only metadata must reuse text runs"
            );
            assert_eq!(adapter.text_runs[&view.node()], run_ids);
            tree.update_and_process_changes(changed, &mut Changes);
            let node = tree.state().focus().unwrap();
            assert_eq!(node.document_range().text(), value.get());
            assert_eq!(node.text_selection().unwrap().text(), "a");
            assert_eq!(node.data().value(), Some(value.get().as_str()));
            assert_eq!(node.data().is_read_only(), read_only);
            assert!(node.data().supports_action(Action::SetTextSelection));
            assert_eq!(node.data().supports_action(Action::SetValue), !read_only);
        }
        let idle = adapter.update(
            &ui.scene.borrow(),
            &ui.semantics.borrow(),
            Some(view.node()),
            "Test",
            1.,
        );
        assert!(idle.nodes.is_empty());
        let mut malformed = selection;
        malformed.focus.character_index = usize::MAX;
        assert_eq!(
            adapter.resolve_selection(&malformed).unwrap().2,
            value.get().len()
        );
        view.unmount();
        adapter.update(&ui.scene.borrow(), &ui.semantics.borrow(), None, "Test", 1.);
        assert!(adapter.resolve_selection(&selection).is_none());
    }

    /// A long document reads as a page of its lines around the view, a run
    /// a line, with no value copied whole: an edit sends its line's runs
    /// alone (the lines after it only move), the caret far away brings its
    /// own page, and selections map to the document's offsets.
    /// `text_run_data_on` as it was, a map of every grapheme's edges.
    #[allow(clippy::type_complexity)]
    fn reference_runs(
        value: &str,
        block: bool,
        lines: Option<&[zgx::scene::TextLine]>,
        scale: f64,
    ) -> Option<Vec<TextRunData>> {
        // Each shown grapheme's line and edges; a break (not in a line) sits
        // zero-wide at the end of the line before it.
        let mut edges: HashMap<usize, (usize, f32, f32)> = HashMap::new();
        let mut wrap_starts: HashSet<usize> = HashSet::new();
        if let Some(lines) = lines {
            for (index, line) in lines.iter().enumerate() {
                let shown = value.get(line.range.clone())?;
                for (i, (offset, _)) in shown.grapheme_indices(true).enumerate() {
                    let (a, b) = (*line.carets.get(i)?, *line.carets.get(i + 1)?);
                    edges.insert(line.range.start + offset, (index, a, b));
                }
                if index > 0 && !value[..line.range.start].ends_with(['\n', '\r']) {
                    wrap_starts.insert(line.range.start);
                }
            }
        }
        let line_of_break = |offset: usize| {
            let lines = lines?;
            let index = lines.iter().rposition(|line| line.range.end <= offset)?;
            let x = *lines[index].carets.last()?;
            Some((index, x, x))
        };
        // A run's box and places, from its characters' edges.
        let place = |run: &TextRunData| -> Option<RunGeometry> {
            let lines = lines?;
            let chars: Vec<(usize, f32, f32)> = run.offsets[..run.offsets.len() - 1]
                .iter()
                .map(|offset| {
                    edges
                        .get(offset)
                        .copied()
                        .or_else(|| line_of_break(*offset))
                })
                .collect::<Option<_>>()?;
            let line = &lines[chars.first()?.0];
            let rtl = line.rtl;
            let (left, right) = chars
                .iter()
                .fold((f32::MAX, f32::MIN), |(l, r), (_, a, b)| {
                    (l.min(a.min(*b)), r.max(a.max(*b)))
                });
            let scale_f = scale as f32;
            Some(RunGeometry {
                bounds: accesskit::Rect::new(
                    left as f64 * scale,
                    line.rect.y as f64 * scale,
                    right as f64 * scale,
                    (line.rect.y + line.rect.height) as f64 * scale,
                ),
                positions: chars
                    .iter()
                    .map(|(_, a, b)| {
                        let start = if rtl {
                            right - a.max(*b)
                        } else {
                            a.min(*b) - left
                        };
                        start * scale_f
                    })
                    .collect(),
                widths: chars
                    .iter()
                    .map(|(_, a, b)| (b - a).abs() * scale_f)
                    .collect(),
                rtl,
            })
        };
        let finish = |mut run: TextRunData, wraps: bool| {
            run.geometry = place(&run);
            run.wraps = wraps;
            run
        };
        let word_starts: HashSet<_> = value
            .unicode_word_indices()
            .map(|(offset, _)| offset)
            .collect();
        let mut runs = Vec::new();
        let mut run = TextRunData {
            value: String::new(),
            lengths: Vec::new(),
            offsets: vec![0],
            words: Vec::new(),
            geometry: None,
            wraps: false,
        };
        for (offset, grapheme) in value.grapheme_indices(true) {
            let length = u8::try_from(grapheme.len()).ok()?;
            let wrap = !run.lengths.is_empty() && wrap_starts.contains(&offset);
            if run.lengths.len() == 255 || wrap {
                runs.push(finish(run, wrap));
                run = TextRunData {
                    value: String::new(),
                    lengths: Vec::new(),
                    offsets: vec![offset],
                    words: Vec::new(),
                    geometry: None,
                    wraps: false,
                };
            }
            if word_starts.contains(&offset) {
                run.words.push(run.lengths.len() as u8);
            }
            run.value.push_str(grapheme);
            run.lengths.push(length);
            run.offsets.push(offset + grapheme.len());
            if grapheme.ends_with(['\n', '\r']) {
                runs.push(finish(run, false));
                run = TextRunData {
                    value: String::new(),
                    lengths: Vec::new(),
                    offsets: vec![offset + grapheme.len()],
                    words: Vec::new(),
                    geometry: None,
                    wraps: false,
                };
            }
        }
        if block && !value.is_empty() && !value.ends_with(['\n', '\r']) {
            if run.lengths.len() == 255 {
                runs.push(finish(run, false));
                run = TextRunData {
                    value: String::new(),
                    lengths: Vec::new(),
                    offsets: vec![value.len()],
                    words: Vec::new(),
                    geometry: None,
                    wraps: false,
                };
            }
            run.value.push('\n');
            run.lengths.push(1);
            run.offsets.push(value.len());
        }
        runs.push(finish(run, false));
        Some(runs)
    }

    #[test]
    fn text_runs_from_one_walk_match_a_map_of_every_graphemes_edges() {
        use zgx::scene::{Rect, TextLine};
        // Lines as `Scene::text_lines` gives them: each grapheme's caret.
        fn lines_of(value: &str, wraps: &[usize], rtl: bool) -> Vec<TextLine> {
            let mut lines = Vec::new();
            let mut start = 0;
            let mut ends: Vec<(usize, usize)> = Vec::new();
            for (offset, grapheme) in value.grapheme_indices(true) {
                if grapheme.ends_with(['\n', '\r']) {
                    ends.push((offset, offset + grapheme.len()));
                } else if wraps.contains(&offset) {
                    ends.push((offset, offset));
                }
            }
            ends.push((value.len(), value.len()));
            for (index, (end, next)) in ends.into_iter().enumerate() {
                let shown = &value[start..end];
                let mut x = 4.;
                let mut carets = vec![x];
                for (i, _) in shown.grapheme_indices(true).enumerate() {
                    x += 7. + (i % 3) as f32;
                    carets.push(x);
                }
                if rtl {
                    carets = carets.iter().map(|x| 300. - x).collect();
                }
                lines.push(TextLine {
                    range: start..end,
                    rect: Rect::new(0., index as f32 * 18., 300., 18.),
                    carets,
                    rtl,
                });
                start = next;
            }
            lines
        }
        let long = "word ".repeat(80);
        let cases: Vec<(&str, Vec<usize>)> = vec![
            ("", vec![]),
            ("Hello, world", vec![]),
            ("Hello, world", vec![7]),
            ("one\ntwo\n\nthree", vec![]),
            ("wrapped words here\nand after a break", vec![8, 26]),
            (
                "cafe\u{301} \u{1F469}\u{200D}\u{1F4BB} e\u{301}e\u{301}",
                vec![6],
            ),
            ("line\r\nwindows\r\n", vec![]),
            (&long, vec![100, 250]),
        ];
        let rendered = |runs: Option<Vec<TextRunData>>| {
            runs.map(|runs| {
                runs.into_iter()
                    .map(|run| {
                        let geometry = run.geometry.map(|g| {
                            (
                                (g.bounds.x0, g.bounds.y0, g.bounds.x1, g.bounds.y1),
                                g.positions,
                                g.widths,
                                g.rtl,
                            )
                        });
                        (
                            run.value,
                            run.lengths,
                            run.offsets,
                            run.words,
                            run.wraps,
                            geometry,
                        )
                    })
                    .collect::<Vec<_>>()
            })
        };
        for (value, wraps) in &cases {
            for rtl in [false, true] {
                for block in [false, true] {
                    let lines = lines_of(value, wraps, rtl);
                    for lines in [None, Some(&lines[..])] {
                        assert_eq!(
                            rendered(text_run_data_on(value, block, lines, 2.)),
                            rendered(reference_runs(value, block, lines, 2.)),
                            "{value:?} wraps {wraps:?} rtl {rtl} block {block} lines {}",
                            lines.is_some()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_long_document_reads_as_runs_for_a_page_of_its_lines() {
        struct Changes;
        impl accesskit_consumer::TreeChangeHandler for Changes {
            fn node_added(&mut self, _: &accesskit_consumer::Node) {}
            fn node_updated(&mut self, _: &accesskit_consumer::Node, _: &accesskit_consumer::Node) {
            }
            fn focus_moved(
                &mut self,
                _: Option<&accesskit_consumer::Node>,
                _: Option<&accesskit_consumer::Node>,
            ) {
            }
            fn node_removed(&mut self, _: &accesskit_consumer::Node) {}
        }
        let mut ui = Ui::new(400., 300.);
        let lines: Vec<String> = (0..20_000).map(|i| format!("line {i}")).collect();
        let value = ui.signal(zgx::text::TextValue::from(lines.join("\n").as_str()));
        let view = ui.mount(text_area_value("Document", value.clone()).size(300., 200.));
        ui.prepare_frame();
        ui.input.focus(&ui.scene, Some(view.node()));
        let editor = ui.focused_editor().unwrap();
        editor.editor.borrow_mut().set_selection(0, 0);
        editor.refresh();
        ui.prepare_frame();
        let mut adapter = AccessibilityTree::new();
        let update = |adapter: &mut AccessibilityTree| {
            adapter.update(
                &ui.scene.borrow(),
                &ui.semantics.borrow(),
                Some(view.node()),
                "Test",
                1.,
            )
        };
        let runs = |update: &TreeUpdate| {
            update
                .nodes
                .iter()
                .filter(|(_, n)| n.role() == accesskit::Role::TextRun)
                .count()
        };
        let first = update(&mut adapter);
        let mut tree = accesskit_consumer::Tree::new(first, true);
        let page = tree.state().focus().unwrap().document_range().text();
        assert!(page.starts_with("line 0\nline 1\n"), "{page}");
        let shown = page.lines().count();
        assert!((20..400).contains(&shown), "a page of lines, not {shown}");
        assert!(tree.state().focus().unwrap().data().value().is_none());
        // Typed into line 3: its run goes again, and the editor's selection.
        let start: usize = lines[..3].iter().map(|l| l.len() + 1).sum();
        editor
            .editor
            .borrow_mut()
            .set_selection(start + 4, start + 4);
        editor.paste("X");
        ui.prepare_frame();
        let edited = update(&mut adapter);
        assert_eq!(runs(&edited), 1, "only the edited line's run is sent");
        let selection = *edited
            .nodes
            .iter()
            .find(|(id, _)| *id == edited.focus)
            .unwrap()
            .1
            .text_selection()
            .unwrap();
        tree.update_and_process_changes(edited, &mut Changes);
        let page = tree.state().focus().unwrap().document_range().text();
        assert!(
            page.starts_with("line 0\nline 1\nline 2\nlineX 3\nline 4\n"),
            "{page}"
        );
        let caret = start + 5;
        assert_eq!(
            adapter.resolve_selection(&selection),
            Some((view.node(), caret, caret))
        );
        // The caret ten thousand lines on: the page around it.
        let far: usize = lines[..10_000].iter().map(|l| l.len() + 1).sum::<usize>() + 1;
        editor.editor.borrow_mut().set_selection(far, far);
        editor.refresh();
        ui.prepare_frame();
        let moved = update(&mut adapter);
        let selection = *moved
            .nodes
            .iter()
            .find(|(id, _)| *id == moved.focus)
            .unwrap()
            .1
            .text_selection()
            .unwrap();
        tree.update_and_process_changes(moved, &mut Changes);
        let page = tree.state().focus().unwrap().document_range().text();
        assert!(
            page.contains("\nline 10000\n") && !page.contains("lineX 3"),
            "{page}"
        );
        assert_eq!(
            adapter.resolve_selection(&selection),
            Some((view.node(), far, far))
        );
        assert!(update(&mut adapter).nodes.is_empty(), "idle sends nothing");
    }

    #[test]
    fn static_label_value_and_text_content_update_for_native_names() {
        let mut ui = Ui::new(400., 300.);
        let count = ui.signal(0);
        let read = count.clone();
        let view = ui.mount(text_signal(move || format!("Count: {}", read.get())));
        ui.prepare_frame();
        let mut adapter = AccessibilityTree::new();
        let update = adapter.update(
            &ui.scene.borrow(),
            &ui.semantics.borrow(),
            Some(view.node()),
            "Test",
            1.,
        );
        let native_id = adapter.ids[&view.node()];
        let tree = accesskit_consumer::Tree::new(update, true);
        let node = tree.state().focus().unwrap();
        assert!(node.label_comes_from_value());
        assert_eq!(node.value().as_deref(), Some("Count: 0"));
        assert!(node.supports_text_ranges());
        assert_eq!(node.document_range().text(), "Count: 0");
        assert!(!node.data().supports_action(Action::SetTextSelection));
        let run_ids = adapter.text_runs[&view.node()].clone();
        count.set(1);
        ui.prepare_frame();
        let update = adapter.update(
            &ui.scene.borrow(),
            &ui.semantics.borrow(),
            Some(view.node()),
            "Test",
            1.,
        );
        assert_eq!(
            update
                .nodes
                .iter()
                .find(|(id, _)| *id == native_id)
                .unwrap()
                .1
                .value(),
            Some("Count: 1")
        );
        adapter.reset();
        let tree = accesskit_consumer::Tree::new(
            adapter.update(
                &ui.scene.borrow(),
                &ui.semantics.borrow(),
                Some(view.node()),
                "Test",
                1.,
            ),
            true,
        );
        let node = tree.state().focus().unwrap();
        assert_eq!(node.value().as_deref(), Some("Count: 1"));
        assert_eq!(node.document_range().text(), "Count: 1");
        assert_eq!(adapter.text_runs[&view.node()], run_ids);
    }

    #[test]
    fn equal_value_role_change_from_nontext_creates_text_runs() {
        let mut ui = Ui::new(400., 300.);
        let view = ui.mount(text("same"));
        ui.prepare_frame();
        let mut semantic = zgx::semantics::SemanticNode::new(Role::Image, "same");
        semantic.value = Some("same".into());
        ui.semantics.borrow_mut().set(view.node(), semantic.clone());
        let mut adapter = AccessibilityTree::new();
        adapter.update(&ui.scene.borrow(), &ui.semantics.borrow(), None, "Test", 1.);
        assert!(!adapter.text_runs.contains_key(&view.node()));
        semantic.role = Role::Label;
        ui.semantics.borrow_mut().set(view.node(), semantic);
        adapter.update(&ui.scene.borrow(), &ui.semantics.borrow(), None, "Test", 1.);
        assert!(!adapter.text_runs[&view.node()].is_empty());
        adapter.reset();
        let tree = accesskit_consumer::Tree::new(
            adapter.update(
                &ui.scene.borrow(),
                &ui.semantics.borrow(),
                Some(view.node()),
                "Test",
                1.,
            ),
            true,
        );
        let node = tree.state().focus().unwrap();
        assert!(node.supports_text_ranges());
        assert_eq!(node.document_range().text(), "same");
    }

    #[test]
    fn long_lines_split_bounded_metadata_without_changing_text_or_line_linkage() {
        let value = format!("{}\r\n", "word ".repeat(140));
        let runs = text_run_data(&value, false).unwrap();
        assert_eq!(
            runs.iter().map(|r| r.value.as_str()).collect::<String>(),
            value
        );
        assert!(runs.len() > 2);
        for run in &runs {
            assert!(run.lengths.len() <= 255);
            assert_eq!(
                run.lengths.iter().map(|n| *n as usize).sum::<usize>(),
                run.value.len()
            );
            assert!(
                run.words
                    .iter()
                    .all(|word| (*word as usize) < run.lengths.len())
            );
        }
        assert_eq!(runs[runs.len() - 2].lengths.last(), Some(&2));
        assert!(runs.last().unwrap().value.is_empty());
        let unrepresentable = format!("a{}", "\u{301}".repeat(200));
        assert!(text_run_data(&unrepresentable, false).is_none());
        assert_eq!(text_run_data("", false).unwrap().len(), 1);
    }

    #[test]
    fn mixed_owner_selections_are_rejected_and_oversized_graphemes_keep_value() {
        let mut ui = Ui::new(400., 300.);
        let one = ui.mount(text_area("One", ui.signal("one".to_owned())));
        let two = ui.mount(text_area("Two", ui.signal("two".to_owned())));
        ui.prepare_frame();
        let mut adapter = AccessibilityTree::new();
        adapter.update(&ui.scene.borrow(), &ui.semantics.borrow(), None, "Test", 1.);
        let selection = accesskit::TextSelection {
            anchor: accesskit::TextPosition {
                node: adapter.text_runs[&one.node()][0],
                character_index: 0,
            },
            focus: accesskit::TextPosition {
                node: adapter.text_runs[&two.node()][0],
                character_index: 1,
            },
        };
        assert!(adapter.resolve_selection(&selection).is_none());
        let value = format!("a{}", "\u{301}".repeat(200));
        ui.set_accessible_value(one.node(), &value);
        ui.prepare_frame();
        let update = adapter.update(
            &ui.scene.borrow(),
            &ui.semantics.borrow(),
            Some(one.node()),
            "Test",
            1.,
        );
        let editor = &update
            .nodes
            .iter()
            .find(|(id, _)| *id == update.focus)
            .unwrap()
            .1;
        assert_eq!(editor.value(), Some(value.as_str()));
        assert!(!editor.supports_action(Action::SetTextSelection));
        assert!(!adapter.text_runs.contains_key(&one.node()));
    }
}

#[cfg(test)]
mod projection_key_tests {
    use super::*;
    use zgx::scene::{Color, Effects, Layout, NodeKind, Style, Transform};

    fn fixed(width: f32, height: f32) -> Style {
        Style {
            width: Some(width),
            height: Some(height),
            ..Style::default()
        }
    }

    #[test]
    fn unchanged_editor_skips_unrelated_updates_but_tracks_ancestor_and_geometry() {
        let mut scene = Scene::new(400., 300.);
        let parent = scene.append(
            scene.root(),
            NodeKind::Container(Layout::Column),
            fixed(300., 200.),
        );
        let editor = scene.append(
            parent,
            NodeKind::Rect(Color(0, 0, 0, 255)),
            fixed(200., 40.),
        );
        let label = scene.append(
            parent,
            NodeKind::Rect(Color(0, 0, 0, 255)),
            fixed(100., 20.),
        );
        scene.prepare_layout();
        let mut semantics = Semantics::new();
        semantics.set(
            parent,
            zgx::semantics::SemanticNode::new(Role::Group, "Group"),
        );
        let mut field = zgx::semantics::SemanticNode::new(Role::TextInput, "Editor");
        field.value = Some("a👩‍💻你好".repeat(100));
        field.text_selection = Some((0, 1));
        semantics.set(editor, field);
        semantics.set(
            label,
            zgx::semantics::SemanticNode::new(Role::Label, "Before"),
        );
        let mut adapter = AccessibilityTree::new();
        adapter.update(&scene, &semantics, Some(editor), "Test", 1.);
        let editor_id = adapter.ids[&editor];
        let run_ids = adapter.text_runs[&editor].clone();
        semantics.update(label, |node| node.label = "After".into());
        let changed = adapter.update(&scene, &semantics, Some(editor), "Test", 1.);
        assert!(
            changed
                .nodes
                .iter()
                .all(|(id, _)| *id != editor_id && !run_ids.contains(id))
        );
        assert_eq!(adapter.text_runs[&editor], run_ids);
        let focused = adapter.update(&scene, &semantics, Some(label), "Test", 1.);
        assert!(focused.nodes.is_empty());
        assert_eq!(focused.focus, adapter.ids[&label]);

        semantics.update(parent, |node| node.disabled = true);
        let changed = adapter.update(&scene, &semantics, None, "Test", 1.);
        assert!(
            changed
                .nodes
                .iter()
                .find(|(id, _)| *id == editor_id)
                .unwrap()
                .1
                .is_disabled()
        );
        scene.set_transform(editor, Transform { x: 12., y: 9. });
        let changed = adapter.update(&scene, &semantics, None, "Test", 2.);
        let native = &changed
            .nodes
            .iter()
            .find(|(id, _)| *id == editor_id)
            .unwrap()
            .1;
        assert_eq!(native.bounds().unwrap().x0, 24.);
        assert_eq!(native.bounds().unwrap().y0, 18.);
        assert_eq!(native.bounds().unwrap().width(), 400.);
    }

    #[test]
    fn children_visibility_and_reconnect_invalidate_projection_revisions() {
        let mut scene = Scene::new(400., 300.);
        let parent = scene.append(
            scene.root(),
            NodeKind::Container(Layout::Column),
            fixed(300., 200.),
        );
        let child = scene.append(
            scene.root(),
            NodeKind::Rect(Color(0, 0, 0, 255)),
            fixed(100., 20.),
        );
        scene.prepare_layout();
        let mut semantics = Semantics::new();
        semantics.set(
            parent,
            zgx::semantics::SemanticNode::new(Role::Group, "Parent"),
        );
        let mut label = zgx::semantics::SemanticNode::new(Role::Label, "Unicode 你好");
        semantics.set(child, label.clone());
        let mut adapter = AccessibilityTree::new();
        adapter.update(&scene, &semantics, None, "Test", 1.);
        let parent_id = adapter.ids[&parent];
        let child_id = adapter.ids[&child];
        label.logical_parent = Some(parent);
        semantics.set(child, label);
        let changed = adapter.update(&scene, &semantics, None, "Test", 1.);
        assert_eq!(
            changed
                .nodes
                .iter()
                .find(|(id, _)| *id == parent_id)
                .unwrap()
                .1
                .children(),
            &[child_id]
        );
        scene.set_effects(
            child,
            Effects {
                opacity: 0.,
                ..Effects::default()
            },
        );
        let hidden = adapter.update(&scene, &semantics, None, "Test", 1.);
        assert!(
            hidden
                .nodes
                .iter()
                .find(|(id, _)| *id == parent_id)
                .unwrap()
                .1
                .children()
                .is_empty()
        );
        assert!(!adapter.projection_revisions.contains_key(&child));
        assert!(!adapter.text_runs.contains_key(&child));
        scene.set_effects(child, Effects::default());
        adapter.update(&scene, &semantics, None, "Test", 1.);
        let visible_id = adapter.ids[&child];
        let runs = adapter.text_runs[&child].clone();
        adapter.reset();
        let reset = adapter.update(&scene, &semantics, None, "Test", 1.);
        assert!(
            reset
                .nodes
                .iter()
                .any(|(id, node)| *id == visible_id && node.value() == Some("Unicode 你好"))
        );
        assert!(
            runs.iter()
                .all(|run| reset.nodes.iter().any(|(id, _)| id == run))
        );
        assert!(
            adapter
                .update(&scene, &semantics, None, "Test", 1.)
                .nodes
                .is_empty()
        );
    }

    #[test]
    fn replacing_semantics_instance_cannot_reuse_coincident_node_revisions() {
        let mut scene = Scene::new(100., 100.);
        let node = scene.append(
            scene.root(),
            NodeKind::Rect(Color(0, 0, 0, 255)),
            fixed(80., 20.),
        );
        scene.prepare_layout();
        let mut adapter = AccessibilityTree::new();
        for label in ["first", "second"] {
            let mut semantics = Semantics::new();
            semantics.set(node, zgx::semantics::SemanticNode::new(Role::Label, label));
            let update = adapter.update(&scene, &semantics, None, "Test", 1.);
            let id = adapter.ids[&node];
            assert_eq!(
                update
                    .nodes
                    .iter()
                    .find(|(native, _)| *native == id)
                    .unwrap()
                    .1
                    .value(),
                Some(label)
            );
        }
    }
}

#[cfg(test)]
mod document_selection_tests {
    use super::*;
    use zgx::{compose::prelude::*, selection::TextPoint, widgets::Ui};

    #[test]
    fn scrolled_text_moves_its_runs_without_shaping_them_again() {
        let mut ui = Ui::new(400., 200.);
        let offset = ui.signal(0_f32);
        let long = "a word ".repeat(400);
        let view = ui.mount(
            scroll(offset.clone()).h(100.).child(
                column()
                    .w(300.)
                    .children([text(long.as_str()).text_wrap(true), text("Second line")]),
            ),
        );
        ui.prepare_frame();
        let scale = 2.;
        let mut adapter = AccessibilityTree::new();
        let update = |adapter: &mut AccessibilityTree, ui: &Ui| {
            adapter.update(
                &ui.scene.borrow(),
                &ui.semantics.borrow(),
                Some(view.node()),
                "Test",
                scale,
            )
        };
        let runs = |update: &accesskit::TreeUpdate| -> Vec<(accesskit::NodeId, accesskit::Rect)> {
            update
                .nodes
                .iter()
                .filter(|(_, node)| node.role() == accesskit::Role::TextRun)
                .map(|(id, node)| (*id, node.bounds().unwrap()))
                .collect()
        };
        let first = runs(&update(&mut adapter, &ui));
        assert!(first.len() > 2, "a wrapped paragraph's runs");
        let shaped = SHAPED.with(|s| s.get());
        offset.set(30.);
        ui.prepare_frame();
        let second = runs(&update(&mut adapter, &ui));
        assert_eq!(SHAPED.with(|s| s.get()), shaped, "nothing shaped again");
        assert_eq!(second.len(), first.len(), "every run went with its text");
        for ((id, was), (moved, now)) in first.iter().zip(&second) {
            assert_eq!(id, moved);
            assert!(
                (was.y0 - now.y0 - 30. * scale).abs() < 0.01,
                "{was:?} to {now:?}"
            );
            assert!((was.x0 - now.x0).abs() < 0.01);
        }
        // Unchanged and unmoved: nothing sent.
        assert!(runs(&update(&mut adapter, &ui)).is_empty());
    }

    #[test]
    fn a_selection_across_labels_is_the_regions_and_assistive_technology_can_set_it() {
        let mut ui = Ui::new(400., 200.);
        let view = ui.mount(
            column()
                .w(300.)
                .selectable(true)
                .children([text("Hello world"), text("Second line")]),
        );
        ui.prepare_frame();
        let root = view.node();
        let labels: Vec<_> = ui.scene.borrow().children(root).to_vec();
        let at = |node, offset| TextPoint { node, offset };
        assert!(ui.select_text(at(labels[0], 6), at(labels[1], 3)));
        let mut adapter = AccessibilityTree::new();
        let update = adapter.update(
            &ui.scene.borrow(),
            &ui.semantics.borrow(),
            Some(root),
            "Test",
            1.,
        );
        let native = adapter.ids[&root];
        let node = &update.nodes.iter().find(|(id, _)| *id == native).unwrap().1;
        assert!(node.supports_action(Action::SetTextSelection));
        let selection = *node
            .text_selection()
            .expect("the region carries the selection");
        assert_eq!(
            adapter.resolve_document_selection(&selection),
            Some(((labels[0], 6), (labels[1], 3)))
        );
        let tree = accesskit_consumer::Tree::new(update, true);
        let region = tree.state().focus().unwrap();
        // What the platform adapters key their text interfaces on (UIA's
        // TextPattern, NSAccessibility's text ranges): a document's.
        assert_eq!(region.role(), accesskit::Role::Document);
        assert!(region.supports_text_ranges(), "platform text APIs see it");
        assert_eq!(
            region.document_range().text(),
            "Hello world\nSecond line\n",
            "each label a block"
        );
        let text = region.text_selection().unwrap().text();
        assert!(
            text.starts_with("world") && text.ends_with("Sec"),
            "{text:?}"
        );
        // Backwards, as assistive technology asks.
        let reversed = accesskit::TextSelection {
            anchor: selection.focus,
            focus: selection.anchor,
        };
        let (anchor, focus) = adapter.resolve_document_selection(&reversed).unwrap();
        assert!(ui.select_accessible_text(anchor, focus));
        assert_eq!(
            ui.text_selection(),
            Some((at(labels[1], 3), at(labels[0], 6)))
        );
        assert_eq!(ui.selected_text(), "world\nSec");
        ui.clear_text_selection();
        assert_eq!(
            ui.semantics.borrow().get(root).unwrap().document_selection,
            None
        );
    }
}

#[cfg(test)]
mod block_break_tests {
    use super::*;
    use zgx::{compose::prelude::*, selection::TextPoint, widgets::Ui};

    #[test]
    fn a_selectable_regions_labels_are_paragraphs_to_assistive_technology() {
        let mut ui = Ui::new(400., 200.);
        let view = ui.mount(
            column().w(300.).children([
                column()
                    .selectable(true)
                    .children([text("Hello world"), text("Second line")]),
                text("Plain"),
            ]),
        );
        ui.prepare_frame();
        let region = ui.scene.borrow().children(view.node())[0];
        let labels: Vec<_> = ui.scene.borrow().children(region).to_vec();
        let at = |node, offset| TextPoint { node, offset };
        // To the first label's end: its text, not its break.
        assert!(ui.select_text(at(labels[0], 6), at(labels[0], 11)));
        let mut adapter = AccessibilityTree::new();
        let update = adapter.update(
            &ui.scene.borrow(),
            &ui.semantics.borrow(),
            Some(region),
            "Test",
            1.,
        );
        let tree = accesskit_consumer::Tree::new(update, true);
        let document = tree.state().focus().unwrap();
        let line = |index| document.line_range_from_index(index).unwrap().text();
        assert_eq!(line(0), "Hello world\n");
        assert_eq!(line(1), "Second line\n");
        assert_eq!(document.text_selection().unwrap().text(), "world");
        // And where it shows: one box, on the first label's line, right of
        // its start.
        let boxes = document.text_selection().unwrap().bounding_boxes();
        let first = ui.scene.borrow().bounds(labels[0]);
        assert_eq!(boxes.len(), 1, "{boxes:?}");
        assert!(
            boxes[0].x0 > first.x as f64 && boxes[0].x1 > boxes[0].x0,
            "{boxes:?}"
        );
        assert!(
            boxes[0].y0 >= first.y as f64 - 0.5,
            "{boxes:?} in {first:?}"
        );
        let selection = document.text_selection().unwrap().to_text_selection();
        assert_eq!(
            adapter.resolve_document_selection(&selection),
            Some(((labels[0], 6), (labels[0], 11)))
        );
        // A word's move crosses the break to the next paragraph's first.
        let mut word = document.text_selection().unwrap();
        let next = word.end().forward_to_word_start();
        word.set_start(next);
        word.set_end(next.forward_to_word_end());
        assert_eq!(word.text().trim_end(), "Second");
        // After a block's break is its end, as assistive technology sets it.
        let after_break = accesskit::TextSelection {
            anchor: selection.anchor,
            focus: accesskit::TextPosition {
                node: adapter.text_runs[&labels[0]][0],
                character_index: 12,
            },
        };
        assert_eq!(
            adapter.resolve_document_selection(&after_break),
            Some(((labels[0], 6), (labels[0], 11)))
        );
        // A label outside one stays as it was.
        let plain = ui.scene.borrow().children(view.node())[1];
        let run = adapter.text_runs[&plain][0];
        assert_eq!(adapter.cache[&run].value(), Some("Plain"));
    }
}

#[cfg(test)]
mod document_role_tests {
    use super::*;
    use zgx::{compose::prelude::*, semantics::Role, widgets::Ui};

    #[test]
    fn a_selectable_region_is_a_document_unless_it_has_a_role_of_its_own() {
        let mut ui = Ui::new(400., 300.);
        let offset = ui.signal(0.);
        let view = ui.mount(
            column()
                .id("page")
                .selectable(true)
                .child(text("Page"))
                .child(
                    scroll(offset)
                        .id("pane")
                        .w(200.)
                        .h(60.)
                        .selectable(true)
                        .child(text("Pane")),
                ),
        );
        ui.prepare_frame();
        let role = |id: &str| {
            let node = view.find(id).unwrap();
            ui.semantics.borrow().get(node).map(|n| n.role)
        };
        assert_eq!(role("page"), Some(Role::Document));
        assert_eq!(
            role("pane"),
            Some(Role::ScrollView),
            "a scroll view stays one"
        );
        // The window stays the tree's root, the document inside it.
        let mut adapter = AccessibilityTree::new();
        let update = adapter.update(&ui.scene.borrow(), &ui.semantics.borrow(), None, "Test", 1.);
        let tree = accesskit_consumer::Tree::new(update, true);
        let root = tree.state().root();
        assert_eq!(root.role(), accesskit::Role::Window);
        assert!(
            root.children()
                .any(|child| child.role() == accesskit::Role::Document)
        );
    }
}

/// AT-SPI's bus connection (zbus) runs blocking calls on `blocking`'s pool.
#[cfg(all(test, target_os = "linux"))]
mod idle_pool_tests {
    /// Threads of this process whose name starts with `prefix`.
    fn threads(prefix: &str) -> usize {
        std::fs::read_dir("/proc/self/task")
            .unwrap()
            .filter_map(|task| std::fs::read_to_string(task.ok()?.path().join("comm")).ok())
            .filter(|name| name.starts_with(prefix))
            .count()
    }

    #[test]
    fn the_bus_pool_thread_ends_when_idle() {
        // An idle window has no thread waking to look for work.
        block_on(blocking::unblock(|| ()));
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while threads("blocking-") > 0 {
            assert!(
                std::time::Instant::now() < until,
                "an idle pool thread stayed, waking twice a second"
            );
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }

    fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
        let waker = std::task::Waker::noop();
        let mut cx = std::task::Context::from_waker(waker);
        let mut future = std::pin::pin!(future);
        loop {
            if let std::task::Poll::Ready(value) = future.as_mut().poll(&mut cx) {
                return value;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
}

#[cfg(test)]
mod vocabulary_tests {
    use super::*;
    use zgx::{compose::prelude::*, semantics::Live, widgets::Ui};

    /// Every projected node, after a frame.
    fn project(ui: &mut Ui) -> Vec<Node> {
        ui.prepare_frame();
        let mut adapter = AccessibilityTree::new();
        adapter
            .update(&ui.scene.borrow(), &ui.semantics.borrow(), None, "Test", 1.)
            .nodes
            .into_iter()
            .map(|(_, node)| node)
            .collect()
    }
    fn find(nodes: &[Node], role: accesskit::Role) -> Vec<&Node> {
        nodes.iter().filter(|n| n.role() == role).collect()
    }

    /// A list's items say where they are in it ("2 of 3"), as a browser's
    /// `<li>` does, where the app doesn't say so itself; and again when an
    /// item joins. Markers hidden from assistive technology say nothing.
    #[test]
    fn list_items_say_where_they_are_in_their_list() {
        let mut ui = Ui::new(400., 300.);
        let count = ui.signal(3_usize);
        let read = count.clone();
        ui.mount(column().role(Role::List).child(keyed(
            move || (0..read.get()).collect(),
            |i: usize, _| {
                row()
                    .role(Role::ListItem)
                    .child(text("•").accessible_hidden())
                    .child(text(format!("Item {i}")))
            },
        )));
        let mut adapter = AccessibilityTree::new();
        let mut items = |ui: &mut Ui| {
            ui.prepare_frame();
            let nodes: Vec<Node> = adapter
                .update(&ui.scene.borrow(), &ui.semantics.borrow(), None, "Test", 1.)
                .nodes
                .into_iter()
                .map(|(_, node)| node)
                .collect();
            nodes
                .iter()
                .filter(|n| n.role() == accesskit::Role::ListItem)
                .map(|n| (n.position_in_set(), n.size_of_set()))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            items(&mut ui),
            [(Some(1), Some(3)), (Some(2), Some(3)), (Some(3), Some(3))]
        );
        count.set(4);
        let after = items(&mut ui);
        assert!(after.contains(&(Some(1), Some(4))), "{after:?}");
        assert!(after.contains(&(Some(4), Some(4))), "{after:?}");
    }

    #[test]
    fn headings_say_their_level_and_are_named_by_their_text() {
        let mut ui = Ui::new(400., 300.);
        ui.mount(
            column()
                .child(text("Inbox").heading(1))
                .child(text("Today").heading(2))
                .child(text("A paragraph")),
        );
        let nodes = project(&mut ui);
        let headings = find(&nodes, accesskit::Role::Heading);
        assert_eq!(headings.len(), 2);
        assert_eq!(headings[0].level(), Some(1));
        assert_eq!(headings[0].label(), Some("Inbox"));
        assert_eq!(headings[1].level(), Some(2));
        assert_eq!(find(&nodes, accesskit::Role::Label).len(), 1);
    }

    #[test]
    fn a_heading_holding_text_is_named_by_it_and_reads_it_once() {
        // As markdown's headings are built: a box of styled runs.
        let mut ui = Ui::new(400., 300.);
        ui.mount(
            column()
                .selectable(true)
                .child(row().heading(2).child(text("Notes")))
                .child(text("Body")),
        );
        let nodes = project(&mut ui);
        let heading = find(&nodes, accesskit::Role::Heading)[0];
        assert_eq!(heading.label(), Some("Notes"));
        assert_eq!(heading.value(), None);
        assert_eq!(heading.level(), Some(2));
        // Its text is its child's, once: one run says "Notes".
        let runs = nodes
            .iter()
            .filter(|n| n.role() == accesskit::Role::TextRun && n.value() == Some("Notes\n"))
            .count();
        assert_eq!(runs, 1);
    }

    #[test]
    fn options_follow_their_selection_and_click() {
        let mut ui = Ui::new(400., 300.);
        let chosen = ui.signal(0_usize);
        let option = |i: usize| {
            let read = chosen.clone();
            let write = chosen.clone();
            row()
                .role(Role::ListBoxOption)
                .selected_when(move || read.get() == i)
                .on_click(move || {
                    write.set(i);
                })
                .child(text(format!("Mailbox {i}")))
        };
        ui.mount(
            column()
                .role(Role::ListBox)
                .accessible_label("Mailboxes")
                .child(option(0))
                .child(option(1)),
        );
        let nodes = project(&mut ui);
        let options = find(&nodes, accesskit::Role::ListBoxOption);
        assert_eq!(options.len(), 2);
        assert_eq!(options[0].is_selected(), Some(true));
        assert_eq!(options[1].is_selected(), Some(false));
        assert_eq!(options[1].label(), Some("Mailbox 1"));
        assert!(options[1].supports_action(Action::Click));
        assert_eq!(
            find(&nodes, accesskit::Role::ListBox)[0].label(),
            Some("Mailboxes")
        );
        chosen.set(1);
        let nodes = project(&mut ui);
        let options = find(&nodes, accesskit::Role::ListBoxOption);
        assert_eq!(options[0].is_selected(), Some(false));
        assert_eq!(options[1].is_selected(), Some(true));
    }

    #[test]
    fn live_regions_descriptions_and_hidden_decoration() {
        let mut ui = Ui::new(400., 300.);
        ui.mount(
            column()
                .child(text("3 new messages").live(Live::Polite))
                .child(text("Saved").role(Role::Status))
                .child(
                    button()
                        .accessible_description("Moves it out of the inbox")
                        .child(text("Archive")),
                )
                .child(
                    row()
                        .accessible_hidden()
                        .child(text("●"))
                        .child(text("decor")),
                )
                .child(
                    button()
                        .child(text("●").accessible_hidden())
                        .child(text(""))
                        .child(text("Unread")),
                ),
        );
        let nodes = project(&mut ui);
        let live: Vec<_> = nodes.iter().filter_map(|n| n.live()).collect();
        assert_eq!(live, [accesskit::Live::Polite, accesskit::Live::Polite]);
        let button = find(&nodes, accesskit::Role::Button)[0];
        assert_eq!(button.description(), Some("Moves it out of the inbox"));
        let texts: Vec<_> = find(&nodes, accesskit::Role::Label)
            .iter()
            .filter_map(|n| n.value())
            .collect();
        assert!(!texts.contains(&"●"), "{texts:?}");
        assert!(!texts.contains(&"decor"), "{texts:?}");
        // A name made from text leaves out what is hidden, and empty text.
        let names: Vec<_> = find(&nodes, accesskit::Role::Button)
            .iter()
            .filter_map(|n| n.label())
            .collect();
        assert_eq!(names, ["Archive", "Unread"]);
    }

    #[test]
    fn an_invalid_field_is_flagged_and_says_why() {
        let mut ui = Ui::new(400., 300.);
        ui.set_assistive_technology(true);
        let error = ui.signal(Some("An email is required".to_string()));
        let read = error.clone();
        ui.mount(
            column()
                .child(
                    button()
                        .accessible_description("Your work address")
                        .error_when(move || read.get())
                        .child(text("Email")),
                )
                .child(button().error_when(|| None).child(text("Name"))),
        );
        let nodes = project(&mut ui);
        let buttons = find(&nodes, accesskit::Role::Button);
        assert_eq!(buttons[0].invalid(), Some(accesskit::Invalid::True));
        // The platforms' adapters read no error message: it is said after
        // the description.
        assert_eq!(
            buttons[0].description(),
            Some("Your work address. An email is required")
        );
        assert_eq!(buttons[1].invalid(), None);
        assert_eq!(buttons[1].description(), None);
        error.set(None);
        let nodes = project(&mut ui);
        let buttons = find(&nodes, accesskit::Role::Button);
        assert_eq!(buttons[0].invalid(), None);
        assert_eq!(buttons[0].description(), Some("Your work address"));
    }

    #[test]
    fn a_field_points_at_the_text_showing_its_error() {
        let mut ui = Ui::new(400., 300.);
        ui.set_assistive_technology(true);
        ui.mount(
            column().child(text("Wrong")).child(
                column()
                    .child(
                        button()
                            .error_when(|| Some("Wrong".into()))
                            .child(text("Email")),
                    )
                    .child(text("Wrong")),
            ),
        );
        ui.prepare_frame();
        let nodes = AccessibilityTree::new()
            .update(&ui.scene.borrow(), &ui.semantics.borrow(), None, "Test", 1.)
            .nodes;
        let (_, button) = nodes
            .iter()
            .find(|(_, n)| n.role() == accesskit::Role::Button)
            .unwrap();
        assert_eq!(button.invalid(), Some(accesskit::Invalid::True));
        // The text after it, in what holds it: not the one before.
        let wrong: Vec<_> = nodes
            .iter()
            .filter(|(_, n)| n.role() == accesskit::Role::Label && n.value() == Some("Wrong"))
            .map(|(id, _)| *id)
            .collect();
        assert_eq!(wrong.len(), 2);
        let shown = button.error_message().expect("its error message");
        assert_eq!(shown, wrong[1]);
        assert_eq!(button.described_by(), [shown]);
        // With the repository's adapters, which read the message, it is not
        // said again in the description.
        let said = if cfg!(zgx_accesskit_patched) {
            None
        } else {
            Some("Wrong")
        };
        assert_eq!(button.description(), said);
    }

    #[test]
    fn empty_text_leaves_the_tree_until_it_says_something() {
        let mut ui = Ui::new(400., 300.);
        let count = ui.signal(String::new());
        let read = count.clone();
        ui.mount(
            row()
                .child(text("Inbox"))
                .child(text_signal(move || read.get())),
        );
        let labels = |ui: &mut Ui| -> Vec<String> {
            project(ui)
                .iter()
                .filter(|n| n.role() == accesskit::Role::Label)
                .filter_map(|n| n.value().map(str::to_owned))
                .collect()
        };
        assert_eq!(labels(&mut ui), ["Inbox"]);
        count.set("3".into());
        assert_eq!(labels(&mut ui), ["Inbox", "3"]);
        count.set(String::new());
        assert_eq!(labels(&mut ui), ["Inbox"]);
    }

    #[test]
    fn trees_and_grids_say_they_take_focus() {
        use zgx_components::{GridColumn, GridSource, TreeChildren, TreeItem, data_grid, tree};
        let mut ui = Ui::new(600., 400.);
        let files = |parent: Option<&str>| {
            TreeChildren::Ready(match parent {
                None => vec![TreeItem::leaf("a", "Alpha")],
                Some(_) => Vec::new(),
            })
        };
        let (open, chosen) = (ui.signal(Vec::new()), ui.signal(None));
        let columns = ui.signal(vec![GridColumn::new("name", "Name", 100.)]);
        let (sort, current) = (ui.signal(None), ui.signal(None));
        let source = GridSource::new(|| 3, |r, _| format!("Item {r}"));
        ui.mount(
            row()
                .w(600.)
                .h(300.)
                .child(
                    column()
                        .w(200.)
                        .h_full()
                        .child(tree("Files", files, open, chosen, |_| {})),
                )
                .child(
                    column()
                        .w(200.)
                        .h_full()
                        .child(data_grid("Items", columns, sort, current, source)),
                ),
        );
        let nodes = project(&mut ui);
        // An assistive technology focuses them (AT-SPI's Focusable): their
        // focus then shows on their current item.
        for role in [accesskit::Role::Tree, accesskit::Role::Grid] {
            let found = find(&nodes, role);
            assert_eq!(found.len(), 1, "{role:?}");
            assert!(found[0].supports_action(Action::Focus), "{role:?}");
        }
    }
}

/// What AT-SPI reads of zgx's tree, through the adapter AccessKit's Linux
/// host serves it from (vendor/accesskit_unix/ZGX_PATCH.md).
#[cfg(all(test, target_os = "linux"))]
mod atspi_tests {
    use super::*;
    use accesskit_atspi_common::{
        Adapter, AdapterCallback, AppContext, Event, InterfaceSet, PlatformNode, Role as AtspiRole,
        State, WindowBounds,
    };
    use zgx::{compose::prelude::*, widgets::Ui};
    use zgx_components::{GridColumn, GridSource, TreeChildren, TreeItem, data_grid, tree};

    struct Quiet;
    impl AdapterCallback for Quiet {
        fn register_interfaces(
            &self,
            _: &Adapter,
            _: accesskit_atspi_common::NodeId,
            _: InterfaceSet,
        ) {
        }
        fn unregister_interfaces(
            &self,
            _: &Adapter,
            _: accesskit_atspi_common::NodeId,
            _: InterfaceSet,
        ) {
        }
        fn emit_event(&self, _: &Adapter, _: Event) {}
    }
    struct Ignore;
    impl accesskit::ActionHandler for Ignore {
        fn do_action(&mut self, _: accesskit::ActionRequest) {}
    }

    /// Every node under `node`, depth first.
    fn walk(adapter: &Adapter, node: PlatformNode, out: &mut Vec<PlatformNode>) {
        for index in 0..node.child_count().unwrap() as usize {
            if let Some(child) = node.child_at_index(index).unwrap() {
                walk(adapter, adapter.platform_node(child), out);
            }
        }
        out.push(node);
    }

    #[test]
    fn a_grid_reads_as_a_table_and_a_tree_item_as_open_or_closed() {
        let mut ui = Ui::new(600., 400.);
        let files = |parent: Option<&str>| {
            TreeChildren::Ready(match parent {
                None => vec![
                    TreeItem::parent("a", "Alpha"),
                    TreeItem::parent("b", "Beta"),
                ],
                Some("a") => vec![TreeItem::leaf("a1", "Apple")],
                Some(_) => Vec::new(),
            })
        };
        let (open, chosen) = (ui.signal(vec!["a".to_owned()]), ui.signal(None));
        let columns = ui.signal(vec![
            GridColumn::new("name", "Name", 100.),
            GridColumn::new("size", "Size", 80.),
        ]);
        let (sort, current) = (ui.signal(None), ui.signal(None));
        let source = GridSource::new(|| 500, |r, c| format!("{c} {r}"));
        ui.mount(
            row()
                .w(600.)
                .h(300.)
                .child(
                    column()
                        .w(200.)
                        .h_full()
                        .child(tree("Files", files, open, chosen, |_| {})),
                )
                .child(
                    column()
                        .w(300.)
                        .h_full()
                        .child(data_grid("Items", columns, sort, current, source)),
                ),
        );
        ui.prepare_frame();
        let update = AccessibilityTree::new().update(
            &ui.scene.borrow(),
            &ui.semantics.borrow(),
            None,
            "Test",
            1.,
        );
        let adapter = Adapter::new(
            &AppContext::new(None),
            Quiet,
            update,
            true,
            WindowBounds::default(),
            Ignore,
        );
        let mut nodes = Vec::new();
        walk(
            &adapter,
            adapter.platform_node(adapter.root_id()),
            &mut nodes,
        );
        let named = |role: AtspiRole, name: &str| {
            nodes
                .iter()
                .find(|n| n.role().unwrap() == role && n.name().unwrap() == name)
                .unwrap_or_else(|| panic!("no {role:?} {name}"))
        };
        // The grid: its rows counted though few are built, the header's too.
        let grid = named(AtspiRole::Table, "Items");
        assert_eq!(grid.table_size().unwrap(), (501, 2));
        let header = grid.table_header(1, true).unwrap().unwrap();
        assert_eq!(adapter.platform_node(header).name().unwrap(), "Size");
        assert_eq!(grid.table_description(0, true).unwrap(), "Name");
        // Row 3 (the header is row 0), column 1: its cell, which says where
        // it is and which header is over it.
        let cell = adapter.platform_node(grid.table_cell_at(3, 1).unwrap().unwrap());
        assert_eq!(cell.table_cell_position().unwrap(), (3, 1));
        let table = adapter.platform_node(cell.table_cell_table().unwrap());
        assert_eq!(table.name().unwrap(), "Items");
        assert_eq!(cell.table_cell_headers(true).unwrap(), vec![header]);
        assert_eq!(grid.table_cell_at(400, 0).unwrap(), None, "not built");
        // The tree: Alpha open, Beta closed, Apple a leaf.
        let state = |name| named(AtspiRole::TreeItem, name).state();
        assert!(state("Alpha").contains(State::Expandable | State::Expanded));
        assert!(state("Beta").contains(State::Expandable | State::Collapsed));
        assert!(!state("Apple").contains(State::Expandable));
    }

    #[test]
    fn an_invalid_field_has_its_error_message_as_a_relation() {
        use accesskit_atspi_common::RelationType;
        let mut ui = Ui::new(400., 300.);
        ui.set_assistive_technology(true);
        let error = || Some("An email is required".to_string());
        ui.mount(
            column()
                .child(button().error_when(error).child(text("Email")))
                .child(text("An email is required"))
                .child(
                    button()
                        .error_when(|| Some("Too long".into()))
                        .child(text("Name")),
                ),
        );
        ui.prepare_frame();
        let update = AccessibilityTree::new().update(
            &ui.scene.borrow(),
            &ui.semantics.borrow(),
            None,
            "Test",
            1.,
        );
        let adapter = Adapter::new(
            &AppContext::new(None),
            Quiet,
            update,
            true,
            WindowBounds::default(),
            Ignore,
        );
        let mut nodes = Vec::new();
        walk(
            &adapter,
            adapter.platform_node(adapter.root_id()),
            &mut nodes,
        );
        // The buttons (each holds a label of the same name).
        let named = |name: &str| {
            nodes
                .iter()
                .find(|n| n.role().unwrap() == AtspiRole::Button && n.name().unwrap() == name)
                .unwrap_or_else(|| panic!("no {name}"))
        };
        let email = named("Email");
        assert!(email.state().contains(State::InvalidEntry));
        let relations = email.relation_set(|id| id).unwrap();
        let message = relations[&RelationType::ErrorMessage][0];
        assert_eq!(
            adapter.platform_node(message).role().unwrap(),
            AtspiRole::Label
        );
        assert_eq!(relations[&RelationType::DescribedBy], vec![message]);
        // Shown as its own text: not said again in the description.
        assert_eq!(email.description().unwrap(), "");
        // Not shown anywhere: said in the description, with no relation.
        let name = named("Name");
        assert!(name.state().contains(State::InvalidEntry));
        assert!(name.relation_set(|id| id).unwrap().is_empty());
        assert_eq!(name.description().unwrap(), "Too long");
    }
}

#[cfg(test)]
mod pacing_tests {
    use super::{GEOMETRY_EVERY, GeometryPacer};
    use std::time::{Duration, Instant};

    /// An animation's frames move things every 16 ms: geometry goes out at
    /// most every `GEOMETRY_EVERY`, the last held change at its deadline
    /// once the frames stop; a name or focus change goes out at once,
    /// whenever it comes.
    /// A dashboard's live values rebuild the tree at once for a screen
    /// reader, paced for other clients; focus goes out at once for both.
    #[test]
    fn names_and_values_go_out_at_once_only_while_a_screen_reader_runs() {
        use super::urgent;
        assert!(urgent(true, false, || false));
        assert!(urgent(true, true, || false));
        assert!(urgent(false, true, || true));
        assert!(!urgent(false, true, || false));
        assert!(!urgent(false, false, || true));
        // The system is asked only when names or values changed.
        assert!(urgent(true, false, || unreachable!()));
        assert!(!urgent(false, false, || unreachable!()));
    }

    #[test]
    fn geometry_alone_goes_out_at_most_every_100_ms_and_changes_at_once() {
        let start = Instant::now();
        let at = |ms: u64| start + Duration::from_millis(ms);
        let mut pacer = GeometryPacer::default();
        // The first tree.
        assert!(pacer.update_now(at(0), true, true));
        // Frames moving things: held until 100 ms on.
        assert!(!pacer.update_now(at(16), false, true));
        assert_eq!(pacer.deadline(), Some(at(0) + GEOMETRY_EVERY));
        assert!(!pacer.update_now(at(32), false, true));
        assert!(!pacer.update_now(at(96), false, true));
        assert!(pacer.update_now(at(112), false, true));
        assert_eq!(pacer.deadline(), None);
        // A name changing mid-animation: at once.
        assert!(pacer.update_now(at(128), true, true));
        // The frames stop after one more move; nothing else comes but the
        // wake at its deadline, which sends it.
        assert!(!pacer.update_now(at(144), false, true));
        assert_eq!(pacer.deadline(), Some(at(228)));
        assert!(!pacer.update_now(at(200), false, false));
        assert!(pacer.update_now(at(228), false, false));
        // Then quiet: nothing to send, nothing held.
        assert!(!pacer.update_now(at(400), false, false));
        assert_eq!(pacer.deadline(), None);
        // Geometry after a quiet spell goes out at once.
        assert!(pacer.update_now(at(500), false, true));
        // Focus moving goes out at once, even right after.
        assert!(pacer.update_now(at(501), true, false));
    }
}
