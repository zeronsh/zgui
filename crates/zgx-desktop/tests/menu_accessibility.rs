use accesskit::{Action, Role};
use accesskit_consumer::{Node, Tree, TreeChangeHandler};
use std::{cell::Cell, rc::Rc};
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
        "Menus",
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
fn menu_projects_roles_disabled_actions_and_keyboard_focus_then_restores_anchor() {
    let mut ui = Ui::new(500., 300.);
    let open = ui.signal(false);
    let calls = Rc::new(Cell::new(0));
    let activated = calls.clone();
    let mounted = ui.mount(
        menu(
            "File",
            open.clone(),
            button().id("anchor").child(text("File")),
        )
        .id("menu")
        .child(menu_item("Unavailable").id("disabled").disabled(true))
        .child(menu_item("First").id("first"))
        .child(
            menu_item("Last")
                .id("last")
                .on_click(move || activated.set(activated.get() + 1)),
        ),
    );
    let anchor = mounted.find("anchor").unwrap();
    let first = mounted.find("first").unwrap();
    let last = mounted.find("last").unwrap();
    ui.input.focus(&ui.scene, Some(anchor));
    let mut adapter = AccessibilityTree::new();
    let initial = update(&ui, &mut adapter);
    let anchor_id = initial.focus;
    let native_anchor = initial
        .nodes
        .iter()
        .find(|(id, _)| *id == anchor_id)
        .unwrap();
    assert_eq!(native_anchor.1.has_popup(), Some(accesskit::HasPopup::Menu));
    assert_eq!(native_anchor.1.is_expanded(), Some(false));
    assert!(
        !initial
            .nodes
            .iter()
            .any(|(_, n)| matches!(n.role(), Role::Menu | Role::MenuItem))
    );
    let mut consumer = Tree::new(initial, true);
    open.set(true);
    let shown = update(&ui, &mut adapter);
    let (menu_id, native) = shown
        .nodes
        .iter()
        .find(|(_, n)| n.role() == Role::Menu)
        .unwrap();
    let menu_id = *menu_id;
    assert_eq!(native.label(), Some("File"));
    assert!(!native.is_modal());
    let expanded_anchor = shown.nodes.iter().find(|(id, _)| *id == anchor_id).unwrap();
    assert_eq!(expanded_anchor.1.is_expanded(), Some(true));
    let items: Vec<_> = shown
        .nodes
        .iter()
        .filter(|(_, n)| n.role() == Role::MenuItem)
        .collect();
    assert_eq!(items.len(), 3);
    for (_, item) in &items {
        assert!(item.supports_action(Action::Click));
        assert!(item.supports_action(Action::Focus));
    }
    assert!(
        items
            .iter()
            .find(|(_, n)| n.label() == Some("Unavailable"))
            .unwrap()
            .1
            .is_disabled()
    );
    assert_eq!(adapter.scene_node(shown.focus), Some(first));
    assert!(native.children().contains(&shown.focus));
    consumer.update_and_process_changes(shown, &mut Changes);
    key(&mut ui, Key::ArrowDown);
    let moved = update(&ui, &mut adapter);
    assert_eq!(adapter.scene_node(moved.focus), Some(last));
    let last_id = moved.focus;
    consumer.update_and_process_changes(moved, &mut Changes);
    ui.input.dispatch_to(
        &ui.scene,
        adapter.scene_node(last_id).unwrap(),
        InputEvent::Activate,
    );
    assert_eq!(calls.get(), 1);
    assert!(!open.get());
    let hidden = update(&ui, &mut adapter);
    assert_eq!(hidden.focus, anchor_id);
    let collapsed_anchor = hidden
        .nodes
        .iter()
        .find(|(id, _)| *id == anchor_id)
        .unwrap();
    assert_eq!(collapsed_anchor.1.is_expanded(), Some(false));
    consumer.update_and_process_changes(hidden, &mut Changes);
    assert_eq!(adapter.scene_node(menu_id), None);
    assert_eq!(adapter.scene_node(last_id), None);
    assert_eq!(
        consumer.state().focus().unwrap().label().as_deref(),
        Some("File")
    );
    mounted.unmount();
    consumer.update_and_process_changes(update(&ui, &mut adapter), &mut Changes);
}
#[test]
fn disabled_menu_item_rejects_native_activation_and_live_disable_updates_consumer() {
    let mut ui = Ui::new(400., 300.);
    let open = ui.signal(true);
    let disabled = ui.signal(true);
    let read = disabled.clone();
    let calls = Rc::new(Cell::new(0));
    let clicked = calls.clone();
    let mounted = ui.mount(
        menu("Edit", open.clone(), button().child(text("Edit")))
            .child(
                menu_item("Delete")
                    .id("delete")
                    .disabled_when(move || read.get())
                    .on_click(move || clicked.set(clicked.get() + 1)),
            )
            .child(menu_item("Cancel")),
    );
    let item = mounted.find("delete").unwrap();
    let mut adapter = AccessibilityTree::new();
    let shown = update(&ui, &mut adapter);
    let (native_id, native) = shown
        .nodes
        .iter()
        .find(|(_, n)| n.label() == Some("Delete") && n.role() == Role::MenuItem)
        .unwrap();
    let native_id = *native_id;
    assert!(native.is_disabled());
    let mut consumer = Tree::new(shown, true);
    ui.input.dispatch_to(&ui.scene, item, InputEvent::Activate);
    assert_eq!(calls.get(), 0);
    assert!(open.get());
    disabled.set(false);
    let changed = update(&ui, &mut adapter);
    assert!(
        !changed
            .nodes
            .iter()
            .find(|(id, _)| *id == native_id)
            .unwrap()
            .1
            .is_disabled()
    );
    consumer.update_and_process_changes(changed, &mut Changes);
    assert!(ui.input.focus(&ui.scene, Some(item)));
    disabled.set(true);
    let changed = update(&ui, &mut adapter);
    assert_ne!(adapter.scene_node(changed.focus), Some(item));
    consumer.update_and_process_changes(changed, &mut Changes);
    key(&mut ui, Key::Escape);
    assert!(!open.get());
    consumer.update_and_process_changes(update(&ui, &mut adapter), &mut Changes);
    assert_eq!(adapter.scene_node(native_id), None);
}

#[test]
fn menu_action_can_open_dialog_without_stale_native_focus_or_scope() {
    let mut ui = Ui::new(500., 300.);
    let menu_open = ui.signal(true);
    let dialog_open = ui.signal(false);
    let launch = dialog_open.clone();
    let mounted = ui.mount(
        column()
            .child(
                menu(
                    "Actions",
                    menu_open.clone(),
                    button().id("trigger").child(text("Actions")),
                )
                .child(menu_item("Settings").id("settings").on_click(move || {
                    launch.set(true);
                })),
            )
            .child(
                modal("Settings dialog", dialog_open.clone())
                    .child(button().id("done").child(text("Done"))),
            ),
    );
    let mut adapter = AccessibilityTree::new();
    let initial = update(&ui, &mut adapter);
    let menu_id = initial
        .nodes
        .iter()
        .find(|(_, n)| n.role() == Role::Menu)
        .unwrap()
        .0;
    let mut consumer = Tree::new(initial, true);
    ui.input.dispatch_to(
        &ui.scene,
        mounted.find("settings").unwrap(),
        InputEvent::Activate,
    );
    assert!(!menu_open.get());
    assert!(dialog_open.get());
    let dialog = update(&ui, &mut adapter);
    assert_eq!(adapter.scene_node(dialog.focus), mounted.find("done"));
    assert!(
        dialog
            .nodes
            .iter()
            .any(|(_, n)| n.role() == Role::Dialog && n.is_modal())
    );
    consumer.update_and_process_changes(dialog, &mut Changes);
    assert_eq!(adapter.scene_node(menu_id), None);
    key(&mut ui, Key::Escape);
    assert!(!dialog_open.get());
    assert!(ui.input.focus_scope().is_none());
    consumer.update_and_process_changes(update(&ui, &mut adapter), &mut Changes);
    mounted.unmount();
    consumer.update_and_process_changes(update(&ui, &mut adapter), &mut Changes);
}

#[test]
fn menu_activation_can_unmount_its_owner_without_leaving_native_nodes() {
    use std::cell::RefCell;
    let mut ui = Ui::new(400., 300.);
    let open = ui.signal(true);
    let owner = Rc::new(RefCell::new(None::<zgx::compose::ViewHandle>));
    let dispose = owner.clone();
    let mounted = ui.mount(
        menu("Close", open.clone(), button().child(text("Close"))).child(
            menu_item("Dispose").id("dispose").on_click(move || {
                let handle = dispose.borrow_mut().take().unwrap();
                handle.unmount();
            }),
        ),
    );
    *owner.borrow_mut() = Some(mounted.clone());
    let mut adapter = AccessibilityTree::new();
    let initial = update(&ui, &mut adapter);
    let ids: Vec<_> = initial
        .nodes
        .iter()
        .filter(|(_, node)| matches!(node.role(), Role::Menu | Role::MenuItem))
        .map(|(id, _)| *id)
        .collect();
    let mut consumer = Tree::new(initial, true);
    ui.input.dispatch_to(
        &ui.scene,
        mounted.find("dispose").unwrap(),
        InputEvent::Activate,
    );
    assert!(!mounted.is_mounted());
    assert!(!open.get());
    assert!(ui.input.focus_scope().is_none());
    consumer.update_and_process_changes(update(&ui, &mut adapter), &mut Changes);
    for id in ids {
        assert_eq!(adapter.scene_node(id), None);
    }
}
