use accesskit::{Action, Role};
use accesskit_consumer::{Node, Tree, TreeChangeHandler};
use zgx::{
    compose::prelude::*,
    input::{InputEvent, Key, Modifiers},
    widgets::Ui,
};
use zgx_desktop::accessibility::AccessibilityTree;
#[derive(Default)]
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
        "Overlays",
        2.,
    )
}
#[test]
fn modal_native_projection_tracks_visibility_focus_and_modal_state() {
    let mut ui = Ui::new(600., 400.);
    let open = ui.signal(false);
    let mounted = ui.mount(
        column()
            .child(button().id("trigger").child(text("Open")))
            .child(
                modal("Settings", open.clone())
                    .id("dialog")
                    .size(200., 100.)
                    .child(button().id("save").child(text("Save"))),
            ),
    );
    let trigger = mounted.find("trigger").unwrap();
    let save = mounted.find("save").unwrap();
    ui.input.focus(&ui.scene, Some(trigger));
    let mut adapter = AccessibilityTree::new();
    let initial = update(&ui, &mut adapter);
    assert!(
        !initial
            .nodes
            .iter()
            .any(|(_, node)| node.role() == Role::Dialog)
    );
    let trigger_id = initial.focus;
    let mut consumer = Tree::new(initial, true);
    open.set(true);
    let shown = update(&ui, &mut adapter);
    let (dialog_id, dialog) = shown
        .nodes
        .iter()
        .find(|(_, node)| node.role() == Role::Dialog)
        .unwrap();
    let dialog_id = *dialog_id;
    assert_eq!(dialog.label(), Some("Settings"));
    assert!(dialog.is_modal());
    assert!(dialog.supports_action(Action::Focus));
    assert_eq!(adapter.scene_node(shown.focus), Some(save));
    assert!(dialog.children().contains(&shown.focus));
    consumer.update_and_process_changes(shown, &mut Changes);
    ui.dispatch(InputEvent::KeyDown {
        key: Key::Escape,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    assert!(!open.get());
    let hidden = update(&ui, &mut adapter);
    assert_eq!(hidden.focus, trigger_id);
    consumer.update_and_process_changes(hidden, &mut Changes);
    assert_eq!(adapter.scene_node(dialog_id), None);
    assert_eq!(
        consumer.state().focus().unwrap().label().as_deref(),
        Some("Open")
    );
    open.set(true);
    consumer.update_and_process_changes(update(&ui, &mut adapter), &mut Changes);
    mounted.unmount();
    consumer.update_and_process_changes(update(&ui, &mut adapter), &mut Changes);
    assert_eq!(adapter.scene_node(dialog_id), None);
}
#[test]
fn anchored_popup_projects_modal_dialog_at_anchor_bounds_and_restores_focus() {
    let mut ui = Ui::new(600., 400.);
    let open = ui.signal(false);
    let mounted = ui.mount(
        popover(
            "Options",
            open.clone(),
            button().id("anchor").child(text("Options")),
        )
        .id("popup")
        .size(160., 80.)
        .child(button().id("choice").child(text("Choose"))),
    );
    let anchor = mounted.find("anchor").unwrap();
    ui.input.focus(&ui.scene, Some(anchor));
    let mut adapter = AccessibilityTree::new();
    let initial = update(&ui, &mut adapter);
    let anchor_id = initial.focus;
    let mut consumer = Tree::new(initial, true);
    open.set(true);
    let shown = update(&ui, &mut adapter);
    let (popup_id, popup) = shown
        .nodes
        .iter()
        .find(|(_, node)| node.role() == Role::Dialog)
        .unwrap();
    let popup_id = *popup_id;
    assert!(popup.is_modal());
    let bounds = popup.bounds().unwrap();
    let anchor_bounds = ui.scene.borrow().bounds(anchor);
    assert_eq!(
        bounds.y0,
        (anchor_bounds.y + anchor_bounds.height + 4.) as f64 * 2.
    );
    assert_eq!(bounds.width(), 320.);
    assert!(popup.children().contains(&shown.focus));
    consumer.update_and_process_changes(shown, &mut Changes);
    open.set(false);
    let hidden = update(&ui, &mut adapter);
    assert_eq!(hidden.focus, anchor_id);
    consumer.update_and_process_changes(hidden, &mut Changes);
    assert_eq!(adapter.scene_node(popup_id), None);
}
#[test]
fn ordinary_dialog_role_does_not_imply_modal_interaction() {
    let mut ui = Ui::new(100., 100.);
    let node = ui.container(
        ui.root(),
        zgx::scene::Layout::Column,
        zgx::widgets::fixed(50., 50.),
    );
    ui.semantics.borrow_mut().set(
        node,
        zgx::semantics::SemanticNode::new(zgx::semantics::Role::Dialog, "Nonmodal"),
    );
    let update = update(&ui, &mut AccessibilityTree::new());
    let dialog = update
        .nodes
        .iter()
        .find(|(_, node)| node.role() == Role::Dialog)
        .unwrap();
    assert!(!dialog.1.is_modal());
}

#[test]
fn legacy_focus_trapping_dialog_exports_modal_metadata() {
    let mut ui = Ui::new(300., 200.);
    let parent = ui.root();
    let dialog = zgx::components::Dialog::mount(&mut ui, parent, "Legacy", 100., 80., true);
    dialog.show();
    let update = update(&ui, &mut AccessibilityTree::new());
    let native = update
        .nodes
        .iter()
        .find(|(_, node)| node.role() == Role::Dialog)
        .unwrap();
    assert!(native.1.is_modal());
}
