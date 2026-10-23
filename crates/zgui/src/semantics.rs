//! Platform-independent accessibility information attached to retained nodes.
use crate::scene::{NodeId, Scene};
use std::sync::atomic::{AtomicU64, Ordering};

/// What a node is to assistive technology (ARIA's `role`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// The window's root.
    Window,
    /// A grouping of related nodes.
    Group,
    /// Static text.
    Label,
    /// A push button.
    Button,
    /// A link that opens something.
    Link,
    /// A checkbox; see `SemanticNode::checked` and `mixed`.
    CheckBox,
    /// A slider; see `min`, `max` and `numeric_value`.
    Slider,
    /// A single-line text field.
    TextInput,
    /// A multiline text editor.
    MultilineTextInput,
    /// A progress indicator; see `min`, `max` and `numeric_value`.
    Progress,
    /// A scrolling viewport; see `scroll_axis`.
    ScrollView,
    /// A scrollbar.
    ScrollBar,
    /// A divider that resizes the panes beside it (ARIA's focusable
    /// `separator`, AppKit's split view divider).
    Splitter,
    /// An item of a `List`.
    ListItem,
    /// A dialog; see `modal`.
    Dialog,
    /// A menu of `MenuItem`s.
    Menu,
    /// A menu's command.
    MenuItem,
    /// An image, named by its label.
    Image,
    /// Text read and selected as a document: a selectable region
    /// (`View::selectable`), as a browser's page is.
    Document,
    /// A section's title (`h1`…`h6`): screen readers move between them,
    /// and announce its `level`.
    Heading,
    /// Items read as a list (`ul`, `ol`): its `ListItem`s say where they
    /// are in it.
    List,
    /// A list whose items are chosen (`role="listbox"`): its
    /// `ListBoxOption`s say whether they are `selected`.
    ListBox,
    /// An option of a `ListBox`.
    ListBoxOption,
    /// A row of controls (`role="toolbar"`).
    Toolbar,
    /// Advisory text that changes, such as a count or a saved state
    /// (`role="status"`, politely live).
    Status,
    /// Tabs choosing a panel (`role="tablist"`), each a `Tab` that says
    /// whether it is `selected`.
    TabList,
    /// A tab of a `TabList`.
    Tab,
}
/// How a change to a node's text is announced (ARIA's `aria-live`): only
/// once the reader is idle, or at once, interrupting it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Live {
    /// Announced once the reader is idle.
    Polite,
    /// Announced at once, interrupting the reader.
    Assertive,
}
/// The single scrolling axis supported by a semantic scroll view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollAxis {
    /// It scrolls across.
    Horizontal,
    /// It scrolls down.
    Vertical,
}
/// Kind of popup exposed by an interactive trigger.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PopupKind {
    /// It opens a menu.
    Menu,
    /// It opens a dialog.
    Dialog,
}
/// A node's accessibility information: its role, name, value and state,
/// which the desktop host projects to the platform's accessibility API.
#[derive(Clone, Debug, PartialEq)]
pub struct SemanticNode {
    /// What it is.
    pub role: Role,
    /// Its accessible name.
    pub label: String,
    /// Its value as text (a text field's contents).
    pub value: Option<String>,
    /// Committed-text UTF-8 byte offsets, preserving anchor/focus direction.
    pub text_selection: Option<(usize, usize)>,
    /// A checkbox's state: on, off, or `None` for not a checkbox.
    pub checked: Option<bool>,
    /// A checkbox neither on nor off (`View::mixed`): what `checked` says
    /// is set aside.
    pub mixed: bool,
    /// Whether it can't be used now.
    pub disabled: bool,
    /// Readable and selectable, but not editable through user input.
    pub read_only: bool,
    /// Whether this dialog traps interaction within its subtree.
    pub modal: bool,
    /// Whether what it opens is open; `None` for nothing to open.
    pub expanded: Option<bool>,
    /// What it opens, if anything.
    pub has_popup: Option<PopupKind>,
    /// Accessible ownership for a portal whose physical parent differs. Native
    /// projection ignores missing, hidden, self-referential or cyclic parents.
    pub logical_parent: Option<NodeId>,
    /// A range's smallest value.
    pub min: Option<f64>,
    /// A range's largest value.
    pub max: Option<f64>,
    /// A range's current value.
    pub numeric_value: Option<f64>,
    /// None preserves the vertical default of legacy scroll views.
    pub scroll_axis: Option<ScrollAxis>,
    /// One-based position of a retained item in a potentially virtual set.
    pub position_in_set: Option<usize>,
    /// Total set size, including items not currently mounted.
    pub size_of_set: Option<usize>,
    /// A text input's placeholder, shown while it is empty.
    pub placeholder: Option<String>,
    /// A selectable region's selection (`View::selectable`): its anchor and
    /// focus, each a label under this node and a UTF-8 offset into that
    /// label's value.
    pub document_selection: Option<((NodeId, usize), (NodeId, usize))>,
    /// The label was given (`View::accessible_label`), not derived from the
    /// node's text: it stays when that text changes.
    pub named: bool,
    /// A heading's level, 1 for the top (`h1`).
    pub level: Option<usize>,
    /// Whether an option, tab or item is the chosen one (`aria-selected`).
    pub selected: Option<bool>,
    /// Announce changes to the text in this subtree (`aria-live`).
    pub live: Option<Live>,
    /// Read after the name, as a hint (`aria-description`).
    pub description: Option<String>,
    /// Left out of the accessibility tree with its subtree (`aria-hidden`):
    /// decoration, or a copy of what is said elsewhere.
    pub hidden: bool,
}
impl SemanticNode {
    /// A node of `role` named `label`, everything else unset.
    pub fn new(role: Role, label: impl Into<String>) -> Self {
        Self {
            role,
            label: label.into(),
            named: false,
            value: None,
            text_selection: None,
            checked: None,
            mixed: false,
            disabled: false,
            read_only: false,
            modal: false,
            expanded: None,
            has_popup: None,
            logical_parent: None,
            min: None,
            max: None,
            numeric_value: None,
            scroll_axis: None,
            position_in_set: None,
            size_of_set: None,
            placeholder: None,
            document_selection: None,
            level: None,
            selected: None,
            live: None,
            description: None,
            hidden: false,
        }
    }
}
/// Opaque change token scoped to one semantic store and one node revision.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SemanticRevision {
    instance: u64,
    revision: u64,
}
struct SemanticEntry {
    node: SemanticNode,
    revision: u64,
}
/// The accessibility information of a document's nodes, with revisions a
/// host uses to send only what changed.
pub struct Semantics {
    nodes: crate::scene::NodeMap<SemanticEntry>,
    revision: u64,
    instance: u64,
}
impl Default for Semantics {
    fn default() -> Self {
        static NEXT_INSTANCE: AtomicU64 = AtomicU64::new(0);
        let instance = NEXT_INSTANCE
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .expect("semantic store identity exhausted");
        Self {
            nodes: Default::default(),
            revision: 0,
            instance,
        }
    }
}
// Publish mutations even when an update callback unwinds. This deliberately
// preserves the callback's partial mutation rather than attempting rollback.
struct SemanticUpdate<'a> {
    entry: &'a mut SemanticEntry,
    global_revision: &'a mut u64,
    before: SemanticNode,
}
impl Drop for SemanticUpdate<'_> {
    fn drop(&mut self) {
        if self.entry.node != self.before {
            *self.global_revision = self.global_revision.wrapping_add(1);
            self.entry.revision = *self.global_revision;
        }
    }
}
impl Semantics {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }
    /// Set `id`'s information; equal information changes nothing.
    pub fn set(&mut self, id: NodeId, node: SemanticNode) {
        if self.get(id) != Some(&node) {
            self.revision = self.revision.wrapping_add(1);
            self.nodes.insert(
                id,
                SemanticEntry {
                    node,
                    revision: self.revision,
                },
            );
        }
    }
    /// `id`'s information, if it has any.
    pub fn get(&self, id: NodeId) -> Option<&SemanticNode> {
        self.nodes.get(&id).map(|entry| &entry.node)
    }
    /// Changes only when this node changes. Removal invalidates the token;
    /// reinsertion and different semantic stores receive distinct tokens.
    pub fn node_revision(&self, id: NodeId) -> Option<SemanticRevision> {
        self.nodes.get(&id).map(|entry| SemanticRevision {
            instance: self.instance,
            revision: entry.revision,
        })
    }
    /// Change `id`'s information in place, if it has any. It counts as
    /// changed only if `f` changed it.
    pub fn update(&mut self, id: NodeId, f: impl FnOnce(&mut SemanticNode)) {
        if let Some(entry) = self.nodes.get_mut(&id) {
            let before = entry.node.clone();
            let update = SemanticUpdate {
                entry,
                global_revision: &mut self.revision,
                before,
            };
            f(&mut update.entry.node);
        }
    }
    /// Update committed editor text, byte selection and read-only state together.
    /// Equal text preserves its allocation; changed text reuses existing capacity.
    /// Other semantic properties are untouched. Missing nodes are ignored.
    pub fn update_text_input(
        &mut self,
        id: NodeId,
        value: &str,
        selection: (usize, usize),
        read_only: bool,
    ) {
        let Some(entry) = self.nodes.get_mut(&id) else {
            return;
        };
        let node = &mut entry.node;
        let value_changed = node.value.as_deref() != Some(value);
        if !value_changed && node.text_selection == Some(selection) && node.read_only == read_only {
            return;
        }
        if value_changed {
            if let Some(stored) = &mut node.value {
                value.clone_into(stored);
            } else {
                node.value = Some(value.to_owned());
            }
        }
        node.text_selection = Some(selection);
        node.read_only = read_only;
        self.revision = self.revision.wrapping_add(1);
        entry.revision = self.revision;
    }
    /// Set a node's label in place: an equal one changes nothing, and a new
    /// one reuses the old one's room (a label bound to changing text).
    pub fn set_label(&mut self, id: NodeId, label: &str) {
        let Some(entry) = self.nodes.get_mut(&id) else {
            return;
        };
        if entry.node.label == label {
            return;
        }
        label.clone_into(&mut entry.node.label);
        self.revision = self.revision.wrapping_add(1);
        entry.revision = self.revision;
    }
    /// Set a text input's placeholder; an equal one changes nothing.
    pub fn set_placeholder(&mut self, id: NodeId, placeholder: Option<&str>) {
        let Some(entry) = self.nodes.get_mut(&id) else {
            return;
        };
        if entry.node.placeholder.as_deref() == placeholder {
            return;
        }
        entry.node.placeholder = placeholder.map(str::to_owned);
        self.revision = self.revision.wrapping_add(1);
        entry.revision = self.revision;
    }
    /// Remove `id`'s information.
    pub fn remove(&mut self, id: NodeId) {
        if self.nodes.remove(&id).is_some() {
            self.revision = self.revision.wrapping_add(1);
            crate::shrink_sparse(&mut self.nodes);
        }
    }
    /// Remove the information of nodes no longer in `scene`.
    pub fn retain_mounted(&mut self, scene: &Scene) {
        let count = self.nodes.len();
        self.nodes.retain(|id, _| scene.contains(*id));
        if self.nodes.len() != count {
            self.revision = self.revision.wrapping_add(1);
        }
    }
    /// Every node's information, in no particular order.
    pub fn iter(&self) -> impl Iterator<Item = (NodeId, &SemanticNode)> {
        self.nodes.iter().map(|(id, entry)| (*id, &entry.node))
    }
    /// Counts changes to the store: unchanged, nothing needs sending.
    pub fn revision(&self) -> u64 {
        self.revision
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{Color, NodeKind, Style};
    #[test]
    fn node_tokens_isolate_mutations_and_survive_retention() {
        let mut scene = Scene::new(100., 100.);
        let root = scene.root();
        let child = scene.append(root, NodeKind::Rect(Color(0, 0, 0, 0)), Style::default());
        let mut semantics = Semantics::new();
        let node = SemanticNode::new(Role::TextInput, "Editor");
        semantics.set(root, node.clone());
        semantics.set(child, SemanticNode::new(Role::Button, "Other"));
        let initial = semantics.node_revision(root).unwrap();
        let other = semantics.node_revision(child).unwrap();
        semantics.set(root, node.clone());
        semantics.update(root, |_| {});
        assert_eq!(semantics.node_revision(root), Some(initial));
        semantics.update_text_input(root, "value", (0, 0), false);
        let text = semantics.node_revision(root).unwrap();
        assert_ne!(text, initial);
        semantics.update_text_input(root, "value", (0, 0), false);
        assert_eq!(semantics.node_revision(root), Some(text));
        semantics.update_text_input(root, "value", (1, 2), false);
        let selection = semantics.node_revision(root).unwrap();
        assert_ne!(selection, text);
        semantics.update_text_input(root, "value", (1, 2), true);
        let read_only = semantics.node_revision(root).unwrap();
        assert_ne!(read_only, selection);
        semantics.update(root, |node| node.disabled = true);
        let disabled = semantics.node_revision(root).unwrap();
        assert_ne!(disabled, read_only);
        assert_eq!(semantics.node_revision(child), Some(other));
        scene.remove(child);
        semantics.retain_mounted(&scene);
        assert_eq!(semantics.node_revision(child), None);
        assert_eq!(semantics.node_revision(root), Some(disabled));
        semantics.remove(root);
        assert_eq!(semantics.node_revision(root), None);
        semantics.set(root, node.clone());
        assert_ne!(semantics.node_revision(root), Some(initial));
        let mut independent = Semantics::new();
        independent.set(root, node);
        assert_eq!(independent.revision(), 1);
        assert_ne!(independent.node_revision(root), Some(initial));
    }
    #[test]
    fn node_token_publishes_partial_update_when_callback_panics() {
        use std::panic::{AssertUnwindSafe, catch_unwind};
        let scene = Scene::new(100., 100.);
        let id = scene.root();
        let mut semantics = Semantics::new();
        semantics.set(id, SemanticNode::new(Role::Button, "Before"));
        let before = semantics.node_revision(id);
        let revision = semantics.revision();
        assert!(
            catch_unwind(AssertUnwindSafe(|| {
                semantics.update(id, |node| {
                    node.label = "After".into();
                    panic!("partial mutation");
                });
            }))
            .is_err()
        );
        assert_eq!(semantics.get(id).unwrap().label, "After");
        assert_ne!(semantics.node_revision(id), before);
        assert_eq!(semantics.revision(), revision + 1);
        let after = semantics.node_revision(id);
        assert!(
            catch_unwind(AssertUnwindSafe(|| {
                semantics.update(id, |_| panic!("no mutation"));
            }))
            .is_err()
        );
        assert_eq!(semantics.node_revision(id), after);
        assert_eq!(semantics.revision(), revision + 1);
    }
    #[test]
    fn text_input_update_reuses_payload_and_changes_revision_once() {
        let mut scene = Scene::new(100., 100.);
        let id = scene.append(
            scene.root(),
            NodeKind::Rect(Color(0, 0, 0, 0)),
            Style::default(),
        );
        let mut semantics = Semantics::new();
        let mut node = SemanticNode::new(Role::MultilineTextInput, "Document");
        node.disabled = true;
        node.modal = true;
        node.checked = Some(true);
        node.expanded = Some(false);
        node.numeric_value = Some(7.);
        let mut stored = String::with_capacity(1024);
        stored.push_str("unchanged");
        node.value = Some(stored);
        semantics.set(id, node.clone());
        let pointer = semantics.get(id).unwrap().value.as_ref().unwrap().as_ptr();
        let capacity = semantics
            .get(id)
            .unwrap()
            .value
            .as_ref()
            .unwrap()
            .capacity();
        let revision = semantics.revision();
        semantics.update_text_input(id, "unchanged", (3, 1), false);
        assert_eq!(semantics.revision(), revision + 1);
        assert_eq!(
            semantics.get(id).unwrap().value.as_ref().unwrap().as_ptr(),
            pointer
        );
        semantics.update_text_input(id, "unchanged", (3, 1), false);
        assert_eq!(semantics.revision(), revision + 1);
        semantics.update_text_input(id, "new", (0, 3), true);
        assert_eq!(semantics.revision(), revision + 2);
        let current = semantics.get(id).unwrap();
        assert_eq!(current.value.as_deref(), Some("new"));
        assert_eq!(current.value.as_ref().unwrap().as_ptr(), pointer);
        assert_eq!(current.value.as_ref().unwrap().capacity(), capacity);
        node.value = Some("new".into());
        node.text_selection = Some((0, 3));
        node.read_only = true;
        assert_eq!(current, &node, "all unrelated fields remain unchanged");
        semantics.remove(id);
        let removed_revision = semantics.revision();
        semantics.update_text_input(id, "missing", (0, 0), false);
        assert_eq!(semantics.revision(), removed_revision);
        assert!(semantics.get(id).is_none());
    }
    #[test]
    fn text_input_update_initializes_missing_value_and_handles_empty_text() {
        let scene = Scene::new(100., 100.);
        let id = scene.root();
        let mut semantics = Semantics::new();
        semantics.set(id, SemanticNode::new(Role::TextInput, "Empty"));
        let revision = semantics.revision();
        semantics.update_text_input(id, "", (0, 0), false);
        assert_eq!(semantics.revision(), revision + 1);
        assert_eq!(semantics.get(id).unwrap().value.as_deref(), Some(""));
        semantics.update_text_input(id, "", (0, 0), false);
        assert_eq!(semantics.revision(), revision + 1);
    }
    #[test]
    fn semantic_updates_are_equal_suppressed_and_unmounted_nodes_removed() {
        let mut s = Scene::new(100., 100.);
        let n = s.append(
            s.root(),
            NodeKind::Rect(Color(0, 0, 0, 255)),
            Style::default(),
        );
        let mut a = Semantics::new();
        let node = SemanticNode::new(Role::Button, "Save");
        a.set(n, node.clone());
        let rev = a.revision();
        a.set(n, node);
        assert_eq!(a.revision(), rev);
        a.update(n, |n| n.disabled = true);
        assert_eq!(a.revision(), rev + 1);
        s.remove(n);
        a.retain_mounted(&s);
        assert!(a.get(n).is_none());
    }
}
