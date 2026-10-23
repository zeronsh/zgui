//! Native accessibility tree translation. Stable IDs survive incremental updates.
use accesskit::{Action, Node, NodeId as AccessibleId, Tree, TreeId, TreeUpdate};
use std::collections::{HashMap, HashSet};
use unicode_segmentation::UnicodeSegmentation;
use zgui::{
    scene::{NodeId, Scene},
    semantics::{Role, ScrollAxis, SemanticRevision, Semantics},
};

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
}
impl AccessibilityTree {
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
    /// The native position of UTF-8 offset `byte` into label `owner`'s runs.
    fn text_position(&self, owner: NodeId, byte: usize) -> Option<accesskit::TextPosition> {
        let ids = self.text_runs.get(&owner)?;
        Some(run_position(ids, &self.text_positions, byte))
    }
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
        let ordered: Vec<_> = scene
            .paint_items()
            .filter(|p| p.effects.opacity > 0.0)
            .filter_map(|p| semantics.get(p.id).map(|s| (p.id, s)))
            .collect();
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
        self.text_runs.retain(|owner, _| visible.contains(owner));
        self.blocks.retain(|owner| visible.contains(owner));
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
            // A label in a selectable region is one of its blocks, which
            // break its text as a browser's paragraphs do.
            let block = semantic.role == Role::Label && scene.is_selectable(id);
            // Reuse the existing cached node's geometry/children instead of
            // retaining another copy in the revision cache. Text runs must still
            // have live cached nodes/positions before omitting this projection.
            if self.projection_revisions.get(&id) == Some(&revision)
                && self.blocks.contains(&id) == block
                && self.cache.get(&native).is_some_and(|cached| {
                    cached.bounds() == Some(bounds)
                        && cached.is_disabled() == disabled
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
            let text_value = if semantic.role == Role::Label {
                Some(semantic.value.as_deref().unwrap_or(&semantic.label))
            } else {
                n.set_label(semantic.label.clone());
                semantic.value.as_deref()
            };
            if let Some(expanded) = semantic.expanded {
                n.set_expanded(expanded);
            }
            if let Some(popup) = semantic.has_popup {
                n.set_has_popup(match popup {
                    zgui::semantics::PopupKind::Menu => accesskit::HasPopup::Menu,
                    zgui::semantics::PopupKind::Dialog => accesskit::HasPopup::Dialog,
                });
            }
            if semantic.modal {
                n.set_modal();
            }
            if let Some(v) = text_value {
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
                Role::Button | Role::Link | Role::CheckBox | Role::MenuItem
            ) {
                n.add_action(Action::Click);
            }
            if matches!(
                semantic.role,
                Role::TextInput | Role::MultilineTextInput | Role::Slider | Role::ScrollBar
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
                    | Role::ListItem
                    | Role::Dialog
                    | Role::Menu
            ) {
                n.add_action(Action::Focus);
            }
            if let Some(position) = semantic.position_in_set {
                n.set_position_in_set(position);
            }
            if let Some(size) = semantic.size_of_set {
                n.set_size_of_set(size);
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
            if matches!(semantic.role, Role::Slider | Role::ScrollBar) {
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
            if matches!(
                semantic.role,
                Role::TextInput | Role::MultilineTextInput | Role::Label
            ) {
                let reusable = self
                    .ids
                    .get(&id)
                    .and_then(|native| self.cache.get(native))
                    .is_some_and(|cached| {
                        cached.role() == semantic_role(semantic) && cached.value() == text_value
                    })
                    && self.blocks.contains(&id) == block
                    && self.text_runs.get(&id).is_none_or(|ids| {
                        ids.iter().all(|run| {
                            self.cache.contains_key(run) && self.text_positions.contains_key(run)
                        })
                    });
                if !reusable {
                    if block {
                        self.blocks.insert(id);
                    } else {
                        self.blocks.remove(&id);
                    }
                    if let Some(runs) = text_run_data(text_value.unwrap_or(""), block) {
                        let ids = self.text_runs.entry(id).or_default();
                        ids.truncate(runs.len());
                        while ids.len() < runs.len() {
                            self.next += 1;
                            ids.push(AccessibleId(self.next));
                        }
                        for (index, run) in runs.iter().enumerate() {
                            let mut text = Node::new(accesskit::Role::TextRun);
                            text.set_value(run.value.clone());
                            text.set_character_lengths(run.lengths.clone());
                            text.set_word_starts(run.words.clone());
                            if index > 0 && !runs[index - 1].value.ends_with(['\n', '\r']) {
                                text.set_previous_on_line(ids[index - 1]);
                            }
                            if index + 1 < runs.len() && !run.value.ends_with(['\n', '\r']) {
                                text.set_next_on_line(ids[index + 1]);
                            }
                            self.text_positions
                                .insert(ids[index], (id, run.offsets.clone()));
                            nodes.push((ids[index], text));
                        }
                    } else {
                        self.text_runs.remove(&id);
                    }
                }
                if let Some(ids) = self.text_runs.get(&id) {
                    let selection_position =
                        |byte: usize| run_position(ids, &self.text_positions, byte);
                    if semantic.role != Role::Label
                        && let Some((anchor, focus)) = semantic.text_selection
                    {
                        n.set_text_selection(accesskit::TextSelection {
                            anchor: selection_position(anchor),
                            focus: selection_position(focus),
                        });
                    }
                    if semantic.role != Role::Label {
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
        tree.toolkit_name = Some("zgui".into());
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
    };
    for (offset, grapheme) in value.grapheme_indices(true) {
        let length = u8::try_from(grapheme.len()).ok()?;
        if run.lengths.len() == 255 {
            runs.push(run);
            run = TextRunData {
                value: String::new(),
                lengths: Vec::new(),
                offsets: vec![offset],
                words: Vec::new(),
            };
        }
        if word_starts.contains(&offset) {
            run.words.push(run.lengths.len() as u8);
        }
        run.value.push_str(grapheme);
        run.lengths.push(length);
        run.offsets.push(offset + grapheme.len());
        if grapheme.ends_with(['\n', '\r']) {
            runs.push(run);
            run = TextRunData {
                value: String::new(),
                lengths: Vec::new(),
                offsets: vec![offset + grapheme.len()],
                words: Vec::new(),
            };
        }
    }
    if block && !value.is_empty() && !value.ends_with(['\n', '\r']) {
        if run.lengths.len() == 255 {
            runs.push(run);
            run = TextRunData {
                value: String::new(),
                lengths: Vec::new(),
                offsets: vec![value.len()],
                words: Vec::new(),
            };
        }
        run.value.push('\n');
        run.lengths.push(1);
        run.offsets.push(value.len());
    }
    runs.push(run);
    Some(runs)
}

fn semantic_role(semantic: &zgui::semantics::SemanticNode) -> accesskit::Role {
    if semantic.role == Role::MenuItem && semantic.checked.is_some() {
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
        Role::ListItem => accesskit::Role::ListItem,
        Role::Dialog => accesskit::Role::Dialog,
        Role::Menu => accesskit::Role::Menu,
        Role::MenuItem => accesskit::Role::MenuItem,
        Role::Image => accesskit::Role::Image,
        Role::Document => accesskit::Role::Document,
    }
}

/// Translate native page-scroll actions through the same routing as wheel input.
pub(crate) fn scroll_action(ui: &zgui::widgets::Ui, node: NodeId, action: Action) -> bool {
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
        zgui::input::InputEvent::Scroll {
            x: bounds.x + bounds.width / 2.,
            y: bounds.y + bounds.height / 2.,
            delta_x,
            delta_y,
        },
    );
    true
}

#[cfg(test)]
mod scroll_tests {
    use super::*;
    use zgui::{compose::prelude::*, widgets::Ui};
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
    use zgui::{compose::prelude::*, widgets::Ui};

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
        let mut semantic = zgui::semantics::SemanticNode::new(Role::Image, "same");
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
    use zgui::scene::{Color, Effects, Layout, NodeKind, Style, Transform};

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
            zgui::semantics::SemanticNode::new(Role::Group, "Group"),
        );
        let mut field = zgui::semantics::SemanticNode::new(Role::TextInput, "Editor");
        field.value = Some("a👩‍💻你好".repeat(100));
        field.text_selection = Some((0, 1));
        semantics.set(editor, field);
        semantics.set(
            label,
            zgui::semantics::SemanticNode::new(Role::Label, "Before"),
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
            zgui::semantics::SemanticNode::new(Role::Group, "Parent"),
        );
        let mut label = zgui::semantics::SemanticNode::new(Role::Label, "Unicode 你好");
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
            semantics.set(node, zgui::semantics::SemanticNode::new(Role::Label, label));
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
    use zgui::{compose::prelude::*, selection::TextPoint, widgets::Ui};

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
    use zgui::{compose::prelude::*, selection::TextPoint, widgets::Ui};

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
    use zgui::{compose::prelude::*, semantics::Role, widgets::Ui};

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
