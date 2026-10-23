//! Keyed dynamic children with retained node identity and scoped disposal.
//!
//! A template compiler can lower keyed loops to `reconcile`. Initialization runs
//! only for new keys; reactive bindings in each `ViewScope` update retained rows.
//!
//! ```
//! use std::{cell::RefCell, rc::Rc};
//! use zgui::{keyed::KeyedChildren, reactive::Runtime, scene::{Color, Layout, Scene, Style}};
//! let runtime = Runtime::new();
//! let scene = Rc::new(RefCell::new(Scene::new(640.0, 480.0)));
//! let root = scene.borrow().root();
//! let mut rows = KeyedChildren::mount(&runtime, scene.clone(), root, Layout::Column, Style::default());
//! rows.reconcile([10, 20, 30], |key, scope| {
//!     let label = format!("Row {key}");
//!     scope.text(Style::default(), Color(255, 255, 255, 255), 16.0, move || label.clone());
//! }).unwrap();
//! let retained = rows.get(&20).unwrap().root();
//! rows.reconcile([30, 20], |_, _| unreachable!()).unwrap();
//! assert_eq!(rows.get(&20).unwrap().root(), retained);
//! drop(rows); // Disposes every child subscription and the owned container.
//! assert_eq!(runtime.effect_count(), 0);
//! ```
use crate::{
    reactive::Runtime,
    scene::{Layout, NodeId, Scene, Style},
    view::ViewScope,
};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    hash::Hash,
    rc::Rc,
};

/// Why [`KeyedChildren::reconcile`] failed; nothing was changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReconcileError {
    /// The input contains a repeated key at this zero-based index.
    DuplicateKey {
        /// The zero-based index of the repeated key.
        index: usize,
    },
    /// The managed container was removed externally.
    Unmounted,
    /// External children were inserted into the exclusively managed container.
    ForeignChildren,
}
impl std::fmt::Display for ReconcileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateKey { index } => write!(f, "duplicate child key at index {index}"),
            Self::Unmounted => f.write_str("keyed container is unmounted"),
            Self::ForeignChildren => f.write_str("keyed container contains unmanaged children"),
        }
    }
}
impl std::error::Error for ReconcileError {}

/// What a reconcile did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReconcileReport {
    /// Keys newly mounted.
    pub inserted: usize,
    /// Keys unmounted.
    pub removed: usize,
    /// Keys kept, with their nodes and state.
    pub retained: usize,
    /// Whether the keys differ from the last reconcile's, in membership or order.
    pub reordered: bool,
}

/// Owns a dedicated container and keyed child scopes. Dropping it removes all
/// mounted nodes and subscriptions. Do not hold a borrow of its shared scene
/// while reconciling or dropping it. User initialization runs without a scene
/// borrow, so it may create normal `ViewScope` bindings and descendants.
///
/// Children use an intrinsic column by default; initialize can change the
/// child's style/kind through its root ID and the shared scene if desired.
pub struct KeyedChildren<K: Eq + Hash> {
    runtime: Runtime,
    scene: Rc<RefCell<Scene>>,
    container: ViewScope,
    entries: HashMap<K, ViewScope>,
    order: Vec<K>,
}
impl<K: Eq + Hash + Clone> KeyedChildren<K> {
    /// Mount the managed container, with `layout` and `style`, under `parent`, with
    /// no children.
    pub fn mount(
        runtime: &Runtime,
        scene: Rc<RefCell<Scene>>,
        parent: NodeId,
        layout: Layout,
        style: Style,
    ) -> Self {
        Self {
            runtime: runtime.clone(),
            container: ViewScope::mount(runtime, scene.clone(), parent, layout, style),
            scene,
            entries: HashMap::new(),
            order: Vec::new(),
        }
    }
    /// The managed container.
    pub fn root(&self) -> NodeId {
        self.container.root()
    }
    /// The keys, in child order.
    pub fn keys(&self) -> &[K] {
        &self.order
    }
    /// The scope of child `key`.
    pub fn get(&self, key: &K) -> Option<&ViewScope> {
        self.entries.get(key)
    }
    /// The scope of child `key`, mutably.
    pub fn get_mut(&mut self, key: &K) -> Option<&mut ViewScope> {
        self.entries.get_mut(key)
    }
    /// The container's service scope is inherited by children at creation.
    pub fn services_mut(&mut self) -> &mut crate::reactive::ServiceScope {
        self.container.services_mut()
    }

    /// Rejects duplicate keys before any creation, removal, or callback occurs.
    /// New scopes inherit the container's services. Retained scopes preserve
    /// local state, scene IDs, child scopes, and reactive subscriptions.
    /// If initialization panics, newly created scopes are disposed and the
    /// previous keys and children remain mounted. Initializers must only mutate
    /// their own scope; arbitrary external side effects cannot be rolled back.
    pub fn reconcile(
        &mut self,
        keys: impl IntoIterator<Item = K>,
        mut initialize: impl FnMut(&K, &mut ViewScope),
    ) -> Result<ReconcileReport, ReconcileError> {
        let keys: Vec<K> = keys.into_iter().collect();
        let mut unique = HashSet::with_capacity(keys.len());
        for (index, key) in keys.iter().enumerate() {
            if !unique.insert(key) {
                return Err(ReconcileError::DuplicateKey { index });
            }
        }
        {
            let scene = self.scene.borrow();
            if !scene.contains(self.root()) {
                return Err(ReconcileError::Unmounted);
            }
            let owned: HashSet<_> = self.entries.values().map(ViewScope::root).collect();
            if scene
                .children(self.root())
                .iter()
                .any(|child| !owned.contains(child))
            {
                return Err(ReconcileError::ForeignChildren);
            }
        }
        let mut report = ReconcileReport {
            reordered: self.order != keys,
            ..ReconcileReport::default()
        };
        // Keep committed entries untouched until every initializer succeeds.
        // Both the current scope and earlier staged scopes own their cleanup
        // during unwinding, including subscriptions and retained resources.
        let mut staged = HashMap::new();
        for key in &keys {
            if self
                .entries
                .get(key)
                .is_some_and(|scope| self.scene.borrow().contains(scope.root()))
            {
                report.retained += 1;
                continue;
            }
            let mut scope = ViewScope::mount(
                &self.runtime,
                self.scene.clone(),
                self.root(),
                Layout::Column,
                Style::default(),
            );
            *scope.services_mut() = self.container.services().child();
            initialize(key, &mut scope);
            staged.insert(key.clone(), scope);
            report.inserted += 1;
        }
        self.entries.retain(|key, scope| {
            let retain = unique.contains(key) && self.scene.borrow().contains(scope.root());
            if !retain {
                report.removed += 1;
            }
            retain
        });
        self.entries.extend(staged);
        let nodes: Vec<_> = keys.iter().map(|key| self.entries[key].root()).collect();
        assert!(
            self.scene
                .borrow_mut()
                .reorder_children(self.root(), &nodes),
            "keyed initializer must not mutate container ownership"
        );
        self.order = keys;
        Ok(report)
    }
}
impl<K: Eq + Hash> Drop for KeyedChildren<K> {
    fn drop(&mut self) {
        self.entries.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{Color, NodeKind};
    fn fixture() -> (Runtime, Rc<RefCell<Scene>>, KeyedChildren<u32>) {
        let runtime = Runtime::new();
        let scene = Rc::new(RefCell::new(Scene::new(200.0, 200.0)));
        let root = scene.borrow().root();
        let keyed = KeyedChildren::mount(
            &runtime,
            scene.clone(),
            root,
            Layout::Column,
            Style::default(),
        );
        (runtime, scene, keyed)
    }
    fn initialize(key: &u32, scope: &mut ViewScope) {
        scope.services_mut().provide(*key * 10);
        let label = key.to_string();
        scope.text(
            Style::default(),
            Color(255, 255, 255, 255),
            16.0,
            move || label.clone(),
        );
    }
    #[test]
    fn reorder_retains_node_identity_and_state_and_damages_moved_rows() {
        let (_, scene, mut keyed) = fixture();
        keyed.reconcile([1, 2, 3], initialize).unwrap();
        scene.borrow_mut().flush();
        let node = keyed.get(&1).unwrap().root();
        keyed.get_mut(&1).unwrap().services_mut().provide(123_u32);
        let old = scene.borrow().bounds(node);
        let report = keyed
            .reconcile([3, 2, 1], |_, _| {
                panic!("retained rows must not initialize")
            })
            .unwrap();
        assert_eq!(report.retained, 3);
        assert!(report.reordered);
        assert_eq!(keyed.get(&1).unwrap().root(), node);
        assert_eq!(
            *keyed.get(&1).unwrap().services().get::<u32>().unwrap(),
            123
        );
        let damage = scene.borrow_mut().flush().damage;
        let new = scene.borrow().bounds(node);
        assert_ne!(old, new);
        assert!(damage.iter().any(|rect| rect.intersects(old)));
        assert!(damage.iter().any(|rect| rect.intersects(new)));
    }
    #[test]
    fn removal_and_drop_dispose_subscriptions_and_subtrees() {
        let (runtime, scene, mut keyed) = fixture();
        keyed.reconcile([1, 2, 3], initialize).unwrap();
        let removed = keyed.get(&2).unwrap().root();
        assert_eq!(runtime.effect_count(), 3);
        keyed.reconcile([1, 3], initialize).unwrap();
        assert!(!scene.borrow().contains(removed));
        assert_eq!(runtime.effect_count(), 2);
        drop(keyed);
        assert_eq!(runtime.effect_count(), 0);
        assert_eq!(scene.borrow().len(), 1);
    }
    #[test]
    fn late_initializer_panic_preserves_committed_rows_and_disposes_staged_scopes() {
        use std::{cell::Cell, panic::AssertUnwindSafe};

        struct Guard(Rc<Cell<usize>>);
        impl Drop for Guard {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }

        let (runtime, scene, mut keyed) = fixture();
        keyed.reconcile([1, 2, 3], initialize).unwrap();
        let previous_nodes = scene.borrow().children(keyed.root()).to_vec();
        let previous_len = scene.borrow().len();
        let previous_effects = runtime.effect_count();
        let dropped = Rc::new(Cell::new(0));
        let staged_nodes = Rc::new(RefCell::new(Vec::new()));
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            keyed.reconcile([3, 4, 5], |key, scope| {
                staged_nodes.borrow_mut().push(scope.root());
                scope.retain(Guard(dropped.clone()));
                initialize(key, scope);
                let child = scope.child(Layout::Column, Style::default());
                child.retain(Guard(dropped.clone()));
                initialize(key, child);
                assert_ne!(*key, 5, "late initializer failure");
            })
        }));
        assert!(result.is_err());
        assert_eq!(dropped.get(), 4);
        assert_eq!(keyed.keys(), &[1, 2, 3]);
        assert_eq!(scene.borrow().children(keyed.root()), previous_nodes);
        assert_eq!(scene.borrow().len(), previous_len);
        assert_eq!(runtime.effect_count(), previous_effects);
        assert!(
            staged_nodes
                .borrow()
                .iter()
                .all(|node| !scene.borrow().contains(*node))
        );
        for (key, node) in [1, 2, 3].iter().zip(&previous_nodes) {
            assert_eq!(keyed.get(key).unwrap().root(), *node);
        }

        let report = keyed.reconcile([3, 4, 5], initialize).unwrap();
        assert_eq!(report.inserted, 2);
        assert_eq!(report.removed, 2);
        assert_eq!(report.retained, 1);
        assert_eq!(keyed.get(&3).unwrap().root(), previous_nodes[2]);
        assert_eq!(runtime.effect_count(), 3);
        assert_eq!(scene.borrow().len(), previous_len);
        drop(keyed);
        assert_eq!(runtime.effect_count(), 0);
        assert_eq!(scene.borrow().len(), 1);
    }
    #[test]
    fn duplicates_are_rejected_before_mutation_and_same_order_is_idle() {
        let (_, scene, mut keyed) = fixture();
        keyed.reconcile([1, 2], initialize).unwrap();
        scene.borrow_mut().flush();
        assert_eq!(
            keyed.reconcile([2, 3, 3], |_, _| panic!("must validate first")),
            Err(ReconcileError::DuplicateKey { index: 2 })
        );
        assert_eq!(keyed.keys(), &[1, 2]);
        assert!(scene.borrow_mut().flush().is_idle());
        let report = keyed
            .reconcile([1, 2], |_, _| panic!("no new scopes"))
            .unwrap();
        assert_eq!(
            report,
            ReconcileReport {
                retained: 2,
                ..ReconcileReport::default()
            }
        );
        assert!(scene.borrow_mut().flush().is_idle());
    }
    #[test]
    fn external_removal_does_not_write_reused_nodes_and_unmount_is_reported() {
        let (_, scene, mut keyed) = fixture();
        keyed.reconcile([1], initialize).unwrap();
        let old = keyed.get(&1).unwrap().root();
        scene.borrow_mut().remove(old);
        keyed.reconcile([1], initialize).unwrap();
        assert_ne!(keyed.get(&1).unwrap().root(), old);
        scene.borrow_mut().remove(keyed.root());
        assert_eq!(
            keyed.reconcile([1], initialize),
            Err(ReconcileError::Unmounted)
        );
    }
    #[test]
    fn overlay_reorder_changes_paint_stacking_even_without_geometry_changes() {
        let mut scene = Scene::new(100.0, 100.0);
        let root = scene.root();
        scene.set_kind(root, NodeKind::Container(Layout::Overlay));
        let style = Style {
            width: Some(20.0),
            height: Some(20.0),
            ..Style::default()
        };
        let a = scene.append(root, NodeKind::Rect(Color(255, 0, 0, 255)), style.clone());
        let b = scene.append(root, NodeKind::Rect(Color(0, 255, 0, 255)), style);
        scene.flush();
        assert!(!scene.reorder_children(root, &[a, a]));
        assert!(scene.flush().is_idle());
        assert!(scene.reorder_children(root, &[b, a]));
        assert!(!scene.flush().damage.is_empty());
        assert_eq!(scene.hit_test(5.0, 5.0), Some(a));
    }
}
