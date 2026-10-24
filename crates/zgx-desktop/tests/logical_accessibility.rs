use accesskit::Role;
use accesskit_consumer::Tree;
use zgx::{
    scene::{Effects, Layout},
    semantics::{Role as SemanticRole, SemanticNode},
    widgets::{Ui, fixed},
};
use zgx_desktop::accessibility::AccessibilityTree;
fn node(ui: &mut Ui, label: &str) -> zgx::scene::NodeId {
    let node = ui.container(ui.root(), Layout::Overlay, fixed(50., 50.));
    ui.semantics
        .borrow_mut()
        .set(node, SemanticNode::new(SemanticRole::Menu, label));
    node
}
#[test]
fn logical_parent_connects_portal_without_changing_physical_bounds() {
    let mut ui = Ui::new(400., 300.);
    let parent = node(&mut ui, "Parent");
    let child = node(&mut ui, "Child");
    ui.semantics
        .borrow_mut()
        .update(child, |node| node.logical_parent = Some(parent));
    ui.prepare_frame();
    let bounds = ui.scene.borrow().bounds(child);
    let mut adapter = AccessibilityTree::new();
    let update = adapter.update(&ui.scene.borrow(), &ui.semantics.borrow(), None, "App", 1.);
    let parent_id = update
        .nodes
        .iter()
        .find(|(_, n)| n.label() == Some("Parent"))
        .unwrap()
        .0;
    let child_id = update
        .nodes
        .iter()
        .find(|(_, n)| n.label() == Some("Child"))
        .unwrap()
        .0;
    let child_native = update.nodes.iter().find(|(id, _)| *id == child_id).unwrap();
    assert_eq!(child_native.1.bounds().unwrap().y0, bounds.y as f64);
    let tree = Tree::new(update, true);
    assert_eq!(
        tree.state()
            .node_by_tree_local_id(child_id, accesskit::TreeId::ROOT)
            .unwrap()
            .parent_id(),
        Some(
            tree.state()
                .node_by_tree_local_id(parent_id, accesskit::TreeId::ROOT)
                .unwrap()
                .id()
        )
    );
}
#[test]
fn invalid_hidden_self_and_cyclic_logical_parents_keep_valid_native_tree() {
    let mut ui = Ui::new(400., 400.);
    let a = node(&mut ui, "A");
    let b = node(&mut ui, "B");
    let self_parent = node(&mut ui, "Self");
    let hidden = node(&mut ui, "Hidden");
    let hidden_child = node(&mut ui, "Hidden child");
    let stale = node(&mut ui, "Stale");
    let stale_child = node(&mut ui, "Stale child");
    ui.semantics
        .borrow_mut()
        .update(a, |n| n.logical_parent = Some(b));
    ui.semantics
        .borrow_mut()
        .update(b, |n| n.logical_parent = Some(a));
    ui.semantics
        .borrow_mut()
        .update(self_parent, |n| n.logical_parent = Some(self_parent));
    ui.semantics
        .borrow_mut()
        .update(hidden_child, |n| n.logical_parent = Some(hidden));
    ui.semantics
        .borrow_mut()
        .update(stale_child, |n| n.logical_parent = Some(stale));
    ui.scene.borrow_mut().set_effects(
        hidden,
        Effects {
            opacity: 0.,
            ..Default::default()
        },
    );
    ui.remove(stale);
    ui.prepare_frame();
    let update = AccessibilityTree::new().update(
        &ui.scene.borrow(),
        &ui.semantics.borrow(),
        None,
        "App",
        1.,
    );
    let root = update.tree.as_ref().unwrap().root;
    let ids: Vec<_> = update
        .nodes
        .iter()
        .filter(|(_, n)| matches!(n.label(), Some("Self" | "Hidden child" | "Stale child")))
        .map(|(id, _)| *id)
        .collect();
    assert_eq!(
        update
            .nodes
            .iter()
            .filter(|(_, n)| n.role() == Role::Menu)
            .count(),
        5
    );
    let tree = Tree::new(update, true);
    for id in ids {
        assert_eq!(
            tree.state()
                .node_by_tree_local_id(id, accesskit::TreeId::ROOT)
                .unwrap()
                .parent_id(),
            Some(
                tree.state()
                    .node_by_tree_local_id(root, accesskit::TreeId::ROOT)
                    .unwrap()
                    .id()
            )
        );
    }
}
