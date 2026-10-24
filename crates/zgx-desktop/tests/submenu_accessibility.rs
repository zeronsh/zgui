use accesskit::{HasPopup, Role};
use accesskit_consumer::{Node, Tree, TreeChangeHandler};
use zgx::{
    compose::prelude::*,
    input::{InputEvent, Key, Modifiers},
    widgets::Ui,
};
use zgx_desktop::accessibility::AccessibilityTree;
struct Changes;
impl TreeChangeHandler for Changes {
    fn node_added(&mut self, _: &Node) {}
    fn node_updated(&mut self, _: &Node, _: &Node) {}
    fn focus_moved(&mut self, _: Option<&Node>, _: Option<&Node>) {}
    fn node_removed(&mut self, _: &Node) {}
}
fn update(ui: &Ui, adapter: &mut AccessibilityTree) -> accesskit::TreeUpdate {
    ui.prepare_frame();
    adapter.update(
        &ui.scene.borrow(),
        &ui.semantics.borrow(),
        ui.input.focused(),
        "Submenus",
        1.,
    )
}
fn key(ui: &mut Ui, key: Key) {
    ui.dispatch(InputEvent::KeyDown {
        key,
        modifiers: Modifiers::default(),
        repeat: false,
    });
}
#[test]
fn submenu_exposes_popup_trigger_hierarchy_and_right_left_focus_transitions() {
    let mut ui = Ui::new(600., 400.);
    let open = ui.signal(true);
    let nested = ui.signal(false);
    let mounted = ui.mount(
        menu(
            "File",
            open.clone(),
            button().id("anchor").child(text("File")),
        )
        .child(submenu("Export", nested.clone()).child(menu_item("PDF").id("pdf"))),
    );
    let mut adapter = AccessibilityTree::new();
    let initial = update(&ui, &mut adapter);
    let (trigger_id, trigger) = initial
        .nodes
        .iter()
        .find(|(_, n)| n.role() == Role::MenuItem && n.label() == Some("Export"))
        .unwrap();
    let trigger_id = *trigger_id;
    let trigger_scene = adapter.scene_node(trigger_id).unwrap();
    assert_eq!(trigger.has_popup(), Some(HasPopup::Menu));
    assert_eq!(trigger.is_expanded(), Some(false));
    let parent_menu = initial
        .nodes
        .iter()
        .find(|(_, n)| n.role() == Role::Menu && n.label() == Some("File"))
        .unwrap()
        .0;
    assert_eq!(initial.focus, trigger_id);
    let mut consumer = Tree::new(initial, true);
    key(&mut ui, Key::ArrowRight);
    assert!(nested.get());
    let shown = update(&ui, &mut adapter);
    let (child_id, child) = shown
        .nodes
        .iter()
        .find(|(_, n)| n.role() == Role::Menu && n.label() == Some("Export"))
        .unwrap();
    let child_id = *child_id;
    assert!(child.children().contains(&shown.focus));
    assert_eq!(adapter.scene_node(shown.focus), mounted.find("pdf"));
    assert_eq!(
        shown
            .nodes
            .iter()
            .find(|(id, _)| *id == trigger_id)
            .unwrap()
            .1
            .is_expanded(),
        Some(true)
    );
    consumer.update_and_process_changes(shown, &mut Changes);
    assert_eq!(
        consumer
            .state()
            .node_by_tree_local_id(child_id, accesskit::TreeId::ROOT)
            .unwrap()
            .parent_id(),
        Some(
            consumer
                .state()
                .node_by_tree_local_id(parent_menu, accesskit::TreeId::ROOT)
                .unwrap()
                .id()
        )
    );
    key(&mut ui, Key::ArrowLeft);
    assert!(!nested.get());
    assert!(open.get());
    let closed = update(&ui, &mut adapter);
    assert_eq!(closed.focus, trigger_id);
    consumer.update_and_process_changes(closed, &mut Changes);
    assert_eq!(adapter.scene_node(child_id), None);
    assert_eq!(ui.input.focused(), Some(trigger_scene));
    key(&mut ui, Key::Escape);
    assert!(!open.get());
    consumer.update_and_process_changes(update(&ui, &mut adapter), &mut Changes);
    mounted.unmount();
    consumer.update_and_process_changes(update(&ui, &mut adapter), &mut Changes);
}

#[test]
fn disabling_submenu_removes_native_popup_and_disables_trigger() {
    let mut ui = Ui::new(600., 400.);
    let nested = ui.signal(false);
    let disabled = ui.signal(false);
    let read = disabled.clone();
    let mounted = ui.mount(
        menu("Root", ui.signal(true), button().child(text("Root")))
            .child(
                submenu("More", nested.clone())
                    .trigger_id("more")
                    .disabled_when(move || read.get())
                    .child(menu_item("Action")),
            )
            .child(menu_item("Other")),
    );
    let mut adapter = AccessibilityTree::new();
    let initial = update(&ui, &mut adapter);
    let trigger_id = initial
        .nodes
        .iter()
        .find(|(_, n)| n.role() == Role::MenuItem && n.label() == Some("More"))
        .unwrap()
        .0;
    let mut consumer = Tree::new(initial, true);
    nested.set(true);
    let shown = update(&ui, &mut adapter);
    let popup_id = shown
        .nodes
        .iter()
        .find(|(_, n)| n.role() == Role::Menu && n.label() == Some("More"))
        .unwrap()
        .0;
    consumer.update_and_process_changes(shown, &mut Changes);
    disabled.set(true);
    let hidden = update(&ui, &mut adapter);
    let trigger = hidden
        .nodes
        .iter()
        .find(|(id, _)| *id == trigger_id)
        .unwrap();
    assert!(trigger.1.is_disabled());
    assert_eq!(trigger.1.is_expanded(), Some(false));
    assert!(!nested.get());
    consumer.update_and_process_changes(hidden, &mut Changes);
    assert_eq!(adapter.scene_node(popup_id), None);
    assert_ne!(ui.input.focused(), mounted.find("more"));
    mounted.unmount();
    consumer.update_and_process_changes(update(&ui, &mut adapter), &mut Changes);
    assert_eq!(adapter.scene_node(trigger_id), None);
}
