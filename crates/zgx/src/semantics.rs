//! Platform-independent accessibility information attached to retained nodes.
use crate::scene::{NodeId, Scene};
use std::{
    collections::HashMap,
    sync::atomic::{AtomicU64, Ordering},
};

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
    /// The panel a `Tab` shows (`role="tabpanel"`).
    TabPanel,
    /// One choice of several (`role="radiogroup"`), each a `RadioButton`
    /// that says whether it is `checked`: a segmented control.
    RadioGroup,
    /// A choice of a `RadioGroup`.
    RadioButton,
    /// An on and off switch (`role="switch"`), `checked` when on.
    Switch,
    /// A number field stepped up and down (`role="spinbutton"`), with
    /// `min`, `max` and `numeric_value`.
    SpinButton,
    /// A field with a list of options to pick from (`role="combobox"`):
    /// `expanded` while its list shows.
    ComboBox,
    /// A hierarchy of items (`role="tree"`).
    Tree,
    /// An item of a `Tree`: its `level`, `expanded` when it has children
    /// shown, and `selected`.
    TreeItem,
    /// A table of cells read and moved through by row and column
    /// (`role="grid"`).
    Grid,
    /// A row of a `Grid`.
    Row,
    /// A cell of a `Row`.
    Cell,
    /// A column's header cell, which may sort it.
    ColumnHeader,
    /// A message that matters at once (`role="alert"`, assertively live).
    Alert,
    /// One self-contained piece of a feed or a transcript
    /// (`role="article"`): a message, a post, a comment. Screen readers
    /// move between them.
    Article,
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
/// Which way a column header sorts its column (`aria-sort`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortOrder {
    /// Smallest (first) first.
    Ascending,
    /// Largest (last) first.
    Descending,
}
/// A grid's size, or a row's, cell's or column header's place in it, from
/// 0, counting what is virtual (not built) too (`aria-rowcount`,
/// `aria-rowindex` and the rest).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TablePlace {
    /// A row's, cell's or header's row.
    pub row: Option<usize>,
    /// A cell's or header's column.
    pub column: Option<usize>,
    /// A grid's rows, its header's included.
    pub rows: Option<usize>,
    /// A grid's columns.
    pub columns: Option<usize>,
    /// A column header's sort.
    pub sort: Option<SortOrder>,
    /// A grid whose rows are selected several at once
    /// (`aria-multiselectable`).
    pub multiselectable: bool,
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
    /// A checkable menu item that is one of a group: a radio item.
    pub radio: bool,
    /// The keys that do what it does, as its menu shows them ("⌘C").
    pub shortcut: Option<String>,
    /// The option a combobox's field has highlighted, while focus stays in
    /// the field (`aria-activedescendant`).
    pub active_descendant: Option<NodeId>,
    /// Its place in a grid, or a grid's size (boxed: only grids have one).
    pub table: Option<Box<TablePlace>>,
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
            radio: false,
            shortcut: None,
            active_descendant: None,
            table: None,
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
    /// Slots of nodes removed whose information is not yet freed
    /// (`bury`), a bit each: what they hold reads as gone.
    buried: Vec<u64>,
    /// How many bits of `buried` are set.
    buried_count: usize,
    revision: u64,
    instance: u64,
    /// Long text fields' contents (`long_value`), shared with their editors
    /// instead of copied into `value`: few nodes have one, so no node
    /// carries room for it.
    /// With the lines of each assistive technology is given (`long_lines`).
    long: HashMap<NodeId, (crate::text::Text, std::ops::Range<usize>)>,
    /// What is wrong with fields' values (`set_error`), beside the nodes as
    /// `long` is: few have one.
    errors: HashMap<NodeId, String>,
    /// The text node showing a field's text (`set_text_node`), whose lines
    /// assistive technology is given the geometry of: few nodes have one.
    text_nodes: HashMap<NodeId, NodeId>,
    /// What names a node with no name of its own (`set_labelled_by`): an
    /// unnamed popover's or menu's trigger. Few nodes have one, and their
    /// portal clears it as it goes (`unlabel_within`).
    labelled_by: HashMap<NodeId, NodeId>,
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
            buried: Vec::new(),
            buried_count: 0,
            revision: 0,
            instance,
            long: HashMap::new(),
            errors: HashMap::new(),
            text_nodes: HashMap::new(),
            labelled_by: HashMap::new(),
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
    /// Whether `id`'s information reads as gone (`bury`).
    #[inline]
    fn buried(&self, id: NodeId) -> bool {
        self.buried_count > 0
            && self
                .buried
                .get(id.slot() / 64)
                .is_some_and(|bits| bits & (1 << (id.slot() % 64)) != 0)
    }
    /// The information of `ids`, nodes just removed, reads as gone from now
    /// on; `forget` frees it, one node at a time.
    pub(crate) fn bury(&mut self, ids: &[NodeId]) {
        for id in ids {
            let (word, bit) = (id.slot() / 64, 1 << (id.slot() % 64));
            if word >= self.buried.len() {
                self.buried.resize(word + 1, 0);
            }
            if self.buried[word] & bit == 0 {
                self.buried[word] |= bit;
                self.buried_count += 1;
            }
        }
        self.revision = self.revision.wrapping_add(1);
    }
    /// Free the information of `id`, a node `bury` buried.
    pub(crate) fn forget(&mut self, id: NodeId) {
        if let Some(bits) = self.buried.get_mut(id.slot() / 64) {
            let bit = 1 << (id.slot() % 64);
            if *bits & bit != 0 {
                *bits &= !bit;
                self.buried_count -= 1;
            }
        }
        if self.buried_count == 0 {
            self.buried = Vec::new();
        }
        self.nodes.remove(&id);
        self.long.remove(&id);
        if !self.errors.is_empty() {
            self.errors.remove(&id);
        }
        if !self.text_nodes.is_empty() {
            self.text_nodes.remove(&id);
        }
        crate::shrink_sparse(&mut self.nodes);
    }
    /// Set `id`'s information; equal information changes nothing.
    pub fn set(&mut self, id: NodeId, node: SemanticNode) {
        if self.buried(id) {
            return;
        }
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
        if self.buried(id) {
            return None;
        }
        self.nodes.get(&id).map(|entry| &entry.node)
    }
    /// Changes only when this node changes. Removal invalidates the token;
    /// reinsertion and different semantic stores receive distinct tokens.
    pub fn node_revision(&self, id: NodeId) -> Option<SemanticRevision> {
        if self.buried(id) {
            return None;
        }
        self.nodes.get(&id).map(|entry| SemanticRevision {
            instance: self.instance,
            revision: entry.revision,
        })
    }
    /// Count `id`'s information as changed though none of it did: what a
    /// host adds to the node for assistive technology changed (a page's
    /// own tree, another process's, embedded under it), so it is projected
    /// again. Nothing for a node with no information.
    pub fn touch(&mut self, id: NodeId) {
        if self.buried(id) {
            return;
        }
        if let Some(entry) = self.nodes.get_mut(&id) {
            self.revision = self.revision.wrapping_add(1);
            entry.revision = self.revision;
        }
    }
    /// Change `id`'s information in place, if it has any. It counts as
    /// changed only if `f` changed it.
    pub fn update(&mut self, id: NodeId, f: impl FnOnce(&mut SemanticNode)) {
        if self.buried(id) {
            return;
        }
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
        if self.buried(id) {
            return;
        }
        let Some(entry) = self.nodes.get_mut(&id) else {
            return;
        };
        let node = &mut entry.node;
        let value_changed = node.value.as_deref() != Some(value);
        if !value_changed && node.text_selection == Some(selection) && node.read_only == read_only {
            return;
        }
        let value_changed = self.long.remove(&id).is_some() || value_changed;
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
    /// As `update_text_input` for a long text, shared rather than copied.
    pub fn update_long_text_input(
        &mut self,
        id: NodeId,
        text: &crate::text::Text,
        selection: (usize, usize),
        read_only: bool,
    ) {
        let Some(entry) = self.nodes.get_mut(&id) else {
            return;
        };
        let node = &mut entry.node;
        let same = self
            .long
            .get(&id)
            .is_some_and(|(held, _)| held.ptr_eq(text));
        if same && node.text_selection == Some(selection) && node.read_only == read_only {
            return;
        }
        if !same {
            let lines = self.long.remove(&id).map_or(0..0, |(_, lines)| lines);
            self.long.insert(id, (text.clone(), lines));
            node.value = None;
        }
        node.text_selection = Some(selection);
        node.read_only = read_only;
        self.revision = self.revision.wrapping_add(1);
        entry.revision = self.revision;
    }
    /// A long text field's contents, shared with its editor (a clone is a
    /// pointer) instead of copied into its `value`, which is then `None`.
    pub fn long_value(&self, id: NodeId) -> Option<&crate::text::Text> {
        if self.buried(id) {
            return None;
        }
        self.long.get(&id).map(|(text, _)| text)
    }
    /// The lines of a long text field assistive technology is given: the
    /// lines in view and a page around them (or around the caret, scrolled
    /// out of view), as a browser's editors give a page of a long document.
    pub fn long_lines(&self, id: NodeId) -> Option<std::ops::Range<usize>> {
        if self.buried(id) {
            return None;
        }
        self.long.get(&id).map(|(_, lines)| lines.clone())
    }
    /// The lines of a long text field assistive technology is given
    /// (`long_lines`): as the view scrolls, its text unchanged.
    pub fn update_long_lines(&mut self, id: NodeId, lines: std::ops::Range<usize>) {
        let (Some(entry), Some((_, held))) = (self.nodes.get_mut(&id), self.long.get_mut(&id))
        else {
            return;
        };
        if *held == lines {
            return;
        }
        *held = lines;
        self.revision = self.revision.wrapping_add(1);
        entry.revision = self.revision;
    }
    /// As `update_text_input` for text known unchanged (an editor's text at
    /// the same revision): its selection and read-only state.
    pub fn update_text_selection(
        &mut self,
        id: NodeId,
        selection: (usize, usize),
        read_only: bool,
    ) {
        let Some(entry) = self.nodes.get_mut(&id) else {
            return;
        };
        let node = &mut entry.node;
        if node.text_selection == Some(selection) && node.read_only == read_only {
            return;
        }
        node.text_selection = Some(selection);
        node.read_only = read_only;
        self.revision = self.revision.wrapping_add(1);
        entry.revision = self.revision;
    }
    /// Set a node's label in place: an equal one changes nothing, and a new
    /// one reuses the old one's room (a label bound to changing text).
    pub fn set_label(&mut self, id: NodeId, label: &str) {
        if self.buried(id) {
            return;
        }
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
    /// What is wrong with field `id`'s value (`aria-invalid` and
    /// `aria-errormessage`), or `None` while it is fine; an equal one
    /// changes nothing.
    pub fn set_error(&mut self, id: NodeId, error: Option<&str>) {
        if self.buried(id) {
            return;
        }
        let Some(entry) = self.nodes.get_mut(&id) else {
            return;
        };
        if self.errors.get(&id).map(String::as_str) == error {
            return;
        }
        match error {
            Some(error) => self.errors.insert(id, error.to_owned()),
            None => self.errors.remove(&id),
        };
        self.revision = self.revision.wrapping_add(1);
        entry.revision = self.revision;
    }
    /// Whether any field says what is wrong with its value.
    #[inline]
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
    /// What is wrong with field `id`'s value (`set_error`), if anything.
    #[inline]
    pub fn error(&self, id: NodeId) -> Option<&str> {
        if self.errors.is_empty() || self.buried(id) {
            return None;
        }
        self.errors.get(&id).map(String::as_str)
    }
    /// Set a text input's placeholder; an equal one changes nothing.
    pub fn set_placeholder(&mut self, id: NodeId, placeholder: Option<&str>) {
        if self.buried(id) {
            return;
        }
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
    /// `id`'s text shows in text node `text` (a field's): assistive
    /// technology is given that node's lines' geometry (`Scene::text_lines`)
    /// for `id`'s value, while it shows that value.
    pub fn set_text_node(&mut self, id: NodeId, text: NodeId) {
        self.text_nodes.insert(id, text);
    }
    /// The text node showing `id`'s text (`set_text_node`): itself for a
    /// text node, none for a node whose text is not shown in one.
    pub fn text_node(&self, id: NodeId) -> Option<NodeId> {
        self.text_nodes.get(&id).copied()
    }
    /// `id`, which has no name of its own, is named by `by` (ARIA's
    /// `aria-labelledby`): an unnamed popover by its trigger, read as the
    /// trigger is named whenever assistive technology reads it.
    pub fn set_labelled_by(&mut self, id: NodeId, by: NodeId) {
        if self.buried(id) || self.labelled_by.get(&id) == Some(&by) {
            return;
        }
        let Some(entry) = self.nodes.get_mut(&id) else {
            return;
        };
        self.labelled_by.insert(id, by);
        self.revision = self.revision.wrapping_add(1);
        entry.revision = self.revision;
    }
    /// Forget what names the nodes within `root` (a portal's overlay, as it
    /// goes): the portal clears its own, so no node's removal looks.
    pub(crate) fn unlabel_within(&mut self, scene: &Scene, root: NodeId) {
        if !self.labelled_by.is_empty() {
            self.labelled_by.retain(|id, _| !scene.is_within(*id, root));
        }
    }
    /// What names `id` (`set_labelled_by`), if anything.
    #[inline]
    pub fn labelled_by(&self, id: NodeId) -> Option<NodeId> {
        if self.labelled_by.is_empty() || self.buried(id) {
            return None;
        }
        self.labelled_by.get(&id).copied()
    }
    /// Remove `id`'s information.
    pub fn remove(&mut self, id: NodeId) {
        if self.buried(id) {
            return self.forget(id);
        }
        if self.nodes.remove(&id).is_some() {
            self.long.remove(&id);
            if !self.errors.is_empty() {
                self.errors.remove(&id);
            }
            if !self.text_nodes.is_empty() {
                self.text_nodes.remove(&id);
            }
            self.revision = self.revision.wrapping_add(1);
            crate::shrink_sparse(&mut self.nodes);
        }
    }
    /// Remove the information of nodes no longer in `scene`.
    pub fn retain_mounted(&mut self, scene: &Scene) {
        let count = self.nodes.len();
        self.nodes.retain(|id, _| scene.contains(*id));
        self.long.retain(|id, _| scene.contains(*id));
        if !self.errors.is_empty() {
            self.errors.retain(|id, _| scene.contains(*id));
        }
        if !self.labelled_by.is_empty() {
            self.labelled_by.retain(|id, _| scene.contains(*id));
        }
        if self.nodes.len() != count {
            self.revision = self.revision.wrapping_add(1);
        }
    }
    /// Every node's information, in no particular order.
    pub fn iter(&self) -> impl Iterator<Item = (NodeId, &SemanticNode)> {
        self.nodes
            .iter()
            .filter(|(id, _)| !self.buried(**id))
            .map(|(id, entry)| (*id, &entry.node))
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
    /// What names a node is kept beside it, setting it again changes
    /// nothing, and its portal clears it.
    #[test]
    fn labelled_by_is_kept_beside_the_node_and_its_portal_clears_it() {
        let mut scene = Scene::new(100., 100.);
        let root = scene.root();
        let panel = scene.append(root, NodeKind::Rect(Color(0, 0, 0, 0)), Style::default());
        let trigger = scene.append(root, NodeKind::Rect(Color(0, 0, 0, 0)), Style::default());
        let mut semantics = Semantics::new();
        // A node with no information has nothing to name.
        semantics.set_labelled_by(panel, trigger);
        assert_eq!(semantics.labelled_by(panel), None);
        semantics.set(panel, SemanticNode::new(Role::Dialog, ""));
        let before = semantics.node_revision(panel);
        semantics.set_labelled_by(panel, trigger);
        assert_eq!(semantics.labelled_by(panel), Some(trigger));
        let named = semantics.node_revision(panel);
        assert_ne!(named, before);
        semantics.set_labelled_by(panel, trigger);
        assert_eq!(semantics.node_revision(panel), named);
        semantics.unlabel_within(&scene, root);
        assert_eq!(semantics.labelled_by(panel), None);
        assert!(semantics.labelled_by.is_empty());
    }
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
    /// A node touched (what a host adds to it changed) is projected again:
    /// its token and the store's revision move, its information doesn't;
    /// a node with none is left alone.
    #[test]
    fn a_touched_node_reads_as_changed() {
        let mut s = Scene::new(100., 100.);
        let n = s.append(
            s.root(),
            NodeKind::Rect(Color(0, 0, 0, 255)),
            Style::default(),
        );
        let bare = s.append(
            s.root(),
            NodeKind::Rect(Color(0, 0, 0, 255)),
            Style::default(),
        );
        let mut a = Semantics::new();
        a.set(n, SemanticNode::new(Role::Group, "Web page"));
        let (rev, token) = (a.revision(), a.node_revision(n));
        a.touch(n);
        assert_eq!(a.revision(), rev + 1);
        assert_ne!(a.node_revision(n), token);
        assert_eq!(a.get(n).unwrap().label, "Web page");
        a.touch(bare);
        assert_eq!(a.revision(), rev + 1);
        assert!(a.get(bare).is_none());
    }
}
