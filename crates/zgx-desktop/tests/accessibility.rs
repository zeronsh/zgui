use accesskit::{Action, Role as NativeRole};
use accesskit_consumer::{Node, Tree, TreeChangeHandler as ChangeHandler};
use zgx::{
    input::InputEvent,
    scene::Layout,
    semantics::{Role, SemanticNode},
    widgets::{Ui, fixed},
};
use zgx_desktop::accessibility::AccessibilityTree;
#[derive(Default)]
struct Changes {
    removed: usize,
    updated: usize,
}
impl ChangeHandler for Changes {
    fn node_added(&mut self, _: &Node) {}
    fn node_updated(&mut self, _: &Node, _: &Node) {
        self.updated += 1;
    }
    fn focus_moved(&mut self, _: Option<&Node>, _: Option<&Node>) {}
    fn node_removed(&mut self, _: &Node) {
        self.removed += 1;
    }
}
#[test]
fn consumer_accepts_hierarchy_root_semantics_focus_removal_and_reactivation() {
    let mut ui = Ui::new(800., 600.);
    let root = ui.root();
    ui.semantics
        .borrow_mut()
        .set(root, SemanticNode::new(Role::Window, "root"));
    let group = ui.container(root, Layout::Column, fixed(300., 200.));
    ui.semantics
        .borrow_mut()
        .set(group, SemanticNode::new(Role::Group, "Settings"));
    let plain = ui.container(group, Layout::Column, fixed(200., 100.));
    let first = ui.button(plain, "First", 100., || {});
    let second = ui.button(plain, "Second", 100., || {});
    ui.scene.borrow_mut().flush();
    let mut adapter = AccessibilityTree::new();
    let initial = adapter.update(
        &ui.scene.borrow(),
        &ui.semantics.borrow(),
        Some(first),
        "App",
        2.,
    );
    let first_id = initial.focus;
    let root_id = initial.tree.as_ref().unwrap().root;
    assert_ne!(root_id, first_id);
    assert_eq!(adapter.scene_node(first_id), Some(first));
    let mut consumer = Tree::new(initial, true);
    assert_eq!(consumer.state().focus().unwrap().role(), NativeRole::Button);
    let idle = adapter.update(
        &ui.scene.borrow(),
        &ui.semantics.borrow(),
        Some(first),
        "App",
        2.,
    );
    assert!(idle.nodes.is_empty());
    ui.scene
        .borrow_mut()
        .reorder_children(plain, &[second, first]);
    ui.scene.borrow_mut().flush();
    let reordered = adapter.update(
        &ui.scene.borrow(),
        &ui.semantics.borrow(),
        Some(first),
        "App",
        2.,
    );
    let native_group = reordered
        .nodes
        .iter()
        .find(|(_, node)| node.role() == NativeRole::GenericContainer)
        .unwrap();
    let order: Vec<_> = native_group
        .1
        .children()
        .iter()
        .map(|id| adapter.scene_node(*id).unwrap())
        .collect();
    assert_eq!(order, vec![second, first]);
    consumer.update_and_process_changes(reordered, &mut Changes::default());
    let update = adapter.update(
        &ui.scene.borrow(),
        &ui.semantics.borrow(),
        Some(second),
        "App",
        2.,
    );
    let second_id = update.focus;
    assert!(update.nodes.is_empty());
    consumer.update_and_process_changes(update, &mut Changes::default());
    ui.remove(first);
    ui.scene.borrow_mut().flush();
    let update = adapter.update(
        &ui.scene.borrow(),
        &ui.semantics.borrow(),
        Some(second),
        "App",
        2.,
    );
    let mut changes = Changes::default();
    consumer.update_and_process_changes(update, &mut changes);
    assert_eq!(changes.removed, 1);
    assert_eq!(adapter.scene_node(first_id), None);
    assert_eq!(adapter.scene_node(second_id), Some(second));
    adapter.reset();
    let restored = adapter.update(
        &ui.scene.borrow(),
        &ui.semantics.borrow(),
        Some(second),
        "App",
        2.,
    );
    assert_eq!(restored.focus, second_id);
    let _reconnected = Tree::new(restored, true);
}
#[test]
fn slider_and_editor_accessible_actions_update_values_and_reject_disabled() {
    let mut ui = Ui::new(800., 600.);
    let value = ui.signal(50.);
    let slider = ui.slider(ui.root(), "Volume", value.clone(), 0.0..=100., 200.);
    let text = ui.signal("hello".to_owned());
    let editor = ui.text_input(ui.root(), "Name", text.clone(), 200., false);
    assert!(ui.set_accessible_numeric_value(slider, 150.));
    assert_eq!(value.get(), 100.);
    assert!(!ui.set_accessible_numeric_value(slider, f64::NAN));
    assert_eq!(value.get(), 100.);
    ui.input
        .dispatch_to(&ui.scene, slider, InputEvent::Decrement);
    assert_eq!(value.get(), 99.);
    assert!(ui.set_accessible_value(slider, "25"));
    assert_eq!(value.get(), 25.);
    assert!(ui.set_accessible_value(editor.node, "new\r\nname"));
    assert_eq!(text.get(), "newname");
    ui.set_disabled(slider, true);
    assert!(!ui.set_accessible_numeric_value(slider, 0.));
    assert_eq!(value.get(), 25.);
    ui.runtime.flush();
    ui.scene.borrow_mut().flush();
    let update = AccessibilityTree::new().update(
        &ui.scene.borrow(),
        &ui.semantics.borrow(),
        None,
        "App",
        1.,
    );
    let native = update
        .nodes
        .iter()
        .find(|(_, n)| n.role() == NativeRole::Slider)
        .unwrap();
    assert!(native.1.supports_action(Action::Increment));
    assert_eq!(native.1.numeric_value(), Some(25.));
    assert!(native.1.is_disabled());
    let _consumer = Tree::new(update, true);
}

#[test]
fn hidden_dialog_subtree_leaves_native_tree_and_actions() {
    let mut ui = Ui::new(800., 600.);
    let dialog = ui.container(ui.root(), Layout::Column, fixed(200., 100.));
    ui.semantics
        .borrow_mut()
        .set(dialog, SemanticNode::new(Role::Dialog, "Dialog"));
    let button = ui.button(dialog, "Close", 100., || {});
    ui.scene.borrow_mut().flush();
    let mut adapter = AccessibilityTree::new();
    let initial = adapter.update(
        &ui.scene.borrow(),
        &ui.semantics.borrow(),
        Some(button),
        "App",
        1.,
    );
    let old = initial.focus;
    let mut consumer = Tree::new(initial, true);
    ui.scene.borrow_mut().set_effects(
        dialog,
        zgx::scene::Effects {
            opacity: 0.,
            ..Default::default()
        },
    );
    ui.scene.borrow_mut().flush();
    let update = adapter.update(
        &ui.scene.borrow(),
        &ui.semantics.borrow(),
        Some(button),
        "App",
        1.,
    );
    assert_eq!(update.focus, update.tree.as_ref().unwrap().root);
    assert_eq!(adapter.scene_node(old), None);
    let mut changes = Changes::default();
    consumer.update_and_process_changes(update, &mut changes);
    assert_eq!(changes.removed, 2);
}

#[test]
fn disabled_container_marks_native_descendants_disabled() {
    let mut ui = Ui::new(500., 500.);
    let group = ui.container(ui.root(), Layout::Column, fixed(200., 100.));
    ui.button(group, "Save", 100., || {});
    ui.set_disabled(group, true);
    ui.scene.borrow_mut().flush();
    let update = AccessibilityTree::new().update(
        &ui.scene.borrow(),
        &ui.semantics.borrow(),
        None,
        "App",
        1.,
    );
    let button = update
        .nodes
        .iter()
        .find(|(_, n)| n.role() == NativeRole::Button)
        .unwrap();
    assert!(button.1.is_disabled());
    let _consumer = Tree::new(update, true);
}

#[test]
fn composed_checkbox_exports_live_toggle_state_and_rejects_disabled_activation() {
    use accesskit::Toggled;
    use zgx::compose::prelude::*;
    let mut ui = Ui::new(500., 200.);
    let checked = ui.signal(false);
    let disabled = ui.signal(false);
    let read_disabled = disabled.clone();
    let mounted = ui.mount(
        checkbox("Notifications", checked.clone())
            .disabled_when(move || read_disabled.get())
            .p(12.),
    );
    let root = mounted.node();
    ui.prepare_frame();
    let mut adapter = AccessibilityTree::new();
    let initial = adapter.update(
        &ui.scene.borrow(),
        &ui.semantics.borrow(),
        Some(root),
        "Settings",
        2.,
    );
    let native_id = initial.focus;
    let native = initial
        .nodes
        .iter()
        .find(|(id, _)| *id == native_id)
        .unwrap();
    assert_eq!(native.1.role(), NativeRole::CheckBox);
    assert_eq!(native.1.label(), Some("Notifications"));
    assert_eq!(native.1.toggled(), Some(Toggled::False));
    assert!(native.1.supports_action(Action::Click));
    let mut consumer = Tree::new(initial, true);
    let target = adapter.scene_node(native_id).unwrap();
    ui.input
        .dispatch_to(&ui.scene, target, InputEvent::Activate);
    assert!(checked.get());
    let update = adapter.update(
        &ui.scene.borrow(),
        &ui.semantics.borrow(),
        Some(root),
        "Settings",
        2.,
    );
    assert_eq!(
        update
            .nodes
            .iter()
            .find(|(id, _)| *id == native_id)
            .unwrap()
            .1
            .toggled(),
        Some(Toggled::True)
    );
    consumer.update_and_process_changes(update, &mut Changes::default());
    disabled.set(true);
    checked.set(false);
    ui.input
        .dispatch_to(&ui.scene, target, InputEvent::Activate);
    assert!(!checked.get());
    let update = adapter.update(
        &ui.scene.borrow(),
        &ui.semantics.borrow(),
        None,
        "Settings",
        2.,
    );
    let native = update
        .nodes
        .iter()
        .find(|(id, _)| *id == native_id)
        .unwrap();
    assert!(native.1.is_disabled());
    assert_eq!(native.1.toggled(), Some(Toggled::False));
    consumer.update_and_process_changes(update, &mut Changes::default());
    mounted.unmount();
    let update = adapter.update(
        &ui.scene.borrow(),
        &ui.semantics.borrow(),
        None,
        "Settings",
        2.,
    );
    consumer.update_and_process_changes(update, &mut Changes::default());
    assert_eq!(adapter.scene_node(native_id), None);
}

#[test]
fn composed_slider_exports_range_value_and_accessible_adjustments() {
    use zgx::compose::prelude::*;
    let mut ui = Ui::new(500., 200.);
    let value = ui.signal(25.);
    let mounted = ui.mount(slider("Volume", value.clone(), 0.0..=100.).w(320.));
    let root = mounted.node();
    ui.prepare_frame();
    let mut adapter = AccessibilityTree::new();
    let initial = adapter.update(
        &ui.scene.borrow(),
        &ui.semantics.borrow(),
        Some(root),
        "Settings",
        1.,
    );
    let native_id = initial.focus;
    let native = initial
        .nodes
        .iter()
        .find(|(id, _)| *id == native_id)
        .unwrap();
    assert_eq!(native.1.role(), NativeRole::Slider);
    assert_eq!(native.1.label(), Some("Volume"));
    assert_eq!(native.1.min_numeric_value(), Some(0.));
    assert_eq!(native.1.max_numeric_value(), Some(100.));
    assert_eq!(native.1.numeric_value(), Some(25.));
    assert!(native.1.supports_action(Action::Increment));
    assert!(native.1.supports_action(Action::Decrement));
    assert!(native.1.supports_action(Action::SetValue));
    let mut consumer = Tree::new(initial, true);
    let target = adapter.scene_node(native_id).unwrap();
    ui.input
        .dispatch_to(&ui.scene, target, InputEvent::Increment);
    assert_eq!(value.get(), 26.);
    assert!(ui.set_accessible_numeric_value(target, 75.));
    assert_eq!(value.get(), 75.);
    let update = adapter.update(
        &ui.scene.borrow(),
        &ui.semantics.borrow(),
        Some(root),
        "Settings",
        1.,
    );
    assert_eq!(
        update
            .nodes
            .iter()
            .find(|(id, _)| *id == native_id)
            .unwrap()
            .1
            .numeric_value(),
        Some(75.)
    );
    consumer.update_and_process_changes(update, &mut Changes::default());
    ui.set_disabled(root, true);
    assert!(!ui.set_accessible_numeric_value(root, 10.));
    assert_eq!(value.get(), 75.);
    mounted.unmount();
    let update = adapter.update(
        &ui.scene.borrow(),
        &ui.semantics.borrow(),
        None,
        "Settings",
        1.,
    );
    consumer.update_and_process_changes(update, &mut Changes::default());
    assert_eq!(adapter.scene_node(native_id), None);
}

#[test]
fn composed_progress_exports_read_only_live_numeric_state() {
    use zgx::compose::prelude::*;
    let mut ui = Ui::new(400., 100.);
    let value = ui.signal(0.25);
    let mounted = ui.mount(progress("Download", value.clone()));
    ui.prepare_frame();
    let mut adapter = AccessibilityTree::new();
    let update = adapter.update(
        &ui.scene.borrow(),
        &ui.semantics.borrow(),
        None,
        "Transfers",
        1.,
    );
    let (id, native) = update
        .nodes
        .iter()
        .find(|(_, n)| n.role() == NativeRole::ProgressIndicator)
        .unwrap();
    let id = *id;
    assert_eq!(native.label(), Some("Download"));
    assert_eq!(native.numeric_value(), Some(0.25));
    assert_eq!(native.min_numeric_value(), Some(0.));
    assert_eq!(native.max_numeric_value(), Some(1.));
    for action in [
        Action::Focus,
        Action::Click,
        Action::Increment,
        Action::SetValue,
    ] {
        assert!(!native.supports_action(action));
    }
    let mut consumer = Tree::new(update, true);
    value.set(1.);
    let update = adapter.update(
        &ui.scene.borrow(),
        &ui.semantics.borrow(),
        None,
        "Transfers",
        1.,
    );
    assert_eq!(
        update
            .nodes
            .iter()
            .find(|(n, _)| *n == id)
            .unwrap()
            .1
            .numeric_value(),
        Some(1.)
    );
    consumer.update_and_process_changes(update, &mut Changes::default());
    mounted.unmount();
    let update = adapter.update(
        &ui.scene.borrow(),
        &ui.semantics.borrow(),
        None,
        "Transfers",
        1.,
    );
    consumer.update_and_process_changes(update, &mut Changes::default());
    assert_eq!(adapter.scene_node(id), None);
}

#[test]
fn composed_image_exports_alt_text_and_removes_native_identity() {
    use std::sync::Arc;
    use zgx::{compose::prelude::*, image::ImageData};
    let mut ui = Ui::new(400., 100.);
    let pixels = Arc::new(ImageData::new(2, 2, vec![255; 16]).unwrap());
    let mounted = ui.mount(image("Project diagram", pixels).p(4.).size(100., 60.));
    ui.prepare_frame();
    let mut adapter = AccessibilityTree::new();
    let update = adapter.update(
        &ui.scene.borrow(),
        &ui.semantics.borrow(),
        None,
        "Images",
        2.,
    );
    let (id, native) = update
        .nodes
        .iter()
        .find(|(_, n)| n.role() == NativeRole::Image)
        .unwrap();
    let id = *id;
    assert_eq!(native.label(), Some("Project diagram"));
    assert_eq!(native.bounds().unwrap().width(), 200.);
    assert!(!native.supports_action(Action::Focus));
    let mut consumer = Tree::new(update, true);
    mounted.unmount();
    let update = adapter.update(
        &ui.scene.borrow(),
        &ui.semantics.borrow(),
        None,
        "Images",
        2.,
    );
    consumer.update_and_process_changes(update, &mut Changes::default());
    assert_eq!(adapter.scene_node(id), None);
}

#[test]
fn composed_scroll_exports_page_actions_and_retained_children() {
    use zgx::compose::prelude::*;
    let mut ui = Ui::new(400., 200.);
    let offset = ui.signal(0.);
    let mounted = ui.mount(
        scroll(offset.clone()).size(200., 80.).child(
            column()
                .child(button().child(text("First")))
                .child(div().h(200.)),
        ),
    );
    ui.prepare_frame();
    let mut adapter = AccessibilityTree::new();
    let update = adapter.update(
        &ui.scene.borrow(),
        &ui.semantics.borrow(),
        None,
        "Scroll",
        1.,
    );
    let (id, node) = update
        .nodes
        .iter()
        .find(|(_, n)| n.role() == NativeRole::ScrollView)
        .unwrap();
    let id = *id;
    assert!(node.supports_action(Action::ScrollUp));
    assert!(node.supports_action(Action::ScrollDown));
    assert!(
        update
            .nodes
            .iter()
            .any(|(_, n)| n.role() == NativeRole::Button && n.label() == Some("First"))
    );
    let mut consumer = Tree::new(update, true);
    offset.set(30.);
    ui.prepare_frame();
    let update = adapter.update(
        &ui.scene.borrow(),
        &ui.semantics.borrow(),
        None,
        "Scroll",
        1.,
    );
    consumer.update_and_process_changes(update, &mut Changes::default());
    assert_eq!(adapter.scene_node(id), Some(mounted.node()));
    mounted.unmount();
    let update = adapter.update(
        &ui.scene.borrow(),
        &ui.semantics.borrow(),
        None,
        "Scroll",
        1.,
    );
    consumer.update_and_process_changes(update, &mut Changes::default());
    assert_eq!(adapter.scene_node(id), None);
}

#[test]
fn focus_reveal_updates_native_descendant_bounds_and_focus() {
    use zgx::compose::prelude::*;
    let mut ui = Ui::new(200., 200.);
    let offset = ui.signal(0.);
    let mounted = ui.mount(
        scroll(offset.clone())
            .size(160., 80.)
            .p(8.)
            .child(div().h(120.))
            .child(button().id("target").size(100., 32.).child(text("Reveal"))),
    );
    ui.prepare_frame();
    let target = mounted.find("target").unwrap();
    let mut adapter = AccessibilityTree::new();
    let initial = adapter.update(
        &ui.scene.borrow(),
        &ui.semantics.borrow(),
        None,
        "Focus",
        2.,
    );
    let mut consumer = Tree::new(initial, true);
    assert!(ui.input.focus(&ui.scene, Some(target)));
    ui.prepare_frame();
    assert_eq!(offset.get(), 88.);
    let update = adapter.update(
        &ui.scene.borrow(),
        &ui.semantics.borrow(),
        ui.input.focused(),
        "Focus",
        2.,
    );
    let focus = update.focus;
    let native = update.nodes.iter().find(|(id, _)| *id == focus).unwrap();
    let bounds = native.1.bounds().unwrap();
    assert!(bounds.y0 >= 16. && bounds.y1 <= 144.);
    consumer.update_and_process_changes(update, &mut Changes::default());
    assert_eq!(consumer.state().focus().unwrap().role(), NativeRole::Button);
    assert_eq!(adapter.scene_node(focus), Some(target));
}

#[test]
fn horizontal_scroll_exports_only_horizontal_native_actions() {
    use zgx::compose::prelude::*;
    let mut ui = Ui::new(400., 200.);
    let mounted = ui.mount(scroll_x(ui.signal(0.)).size(200., 80.).child(div().w(500.)));
    ui.prepare_frame();
    let mut adapter = AccessibilityTree::new();
    let update = adapter.update(
        &ui.scene.borrow(),
        &ui.semantics.borrow(),
        None,
        "Horizontal",
        1.,
    );
    let (id, node) = update
        .nodes
        .iter()
        .find(|(_, n)| n.role() == NativeRole::ScrollView)
        .unwrap();
    let id = *id;
    assert!(node.supports_action(Action::ScrollLeft));
    assert!(node.supports_action(Action::ScrollRight));
    assert!(!node.supports_action(Action::ScrollUp));
    assert!(!node.supports_action(Action::ScrollDown));
    let mut consumer = Tree::new(update, true);
    mounted.unmount();
    let update = adapter.update(
        &ui.scene.borrow(),
        &ui.semantics.borrow(),
        None,
        "Horizontal",
        1.,
    );
    consumer.update_and_process_changes(update, &mut Changes::default());
    assert_eq!(adapter.scene_node(id), None);
}

#[test]
fn overlay_scrollbar_exports_value_actions_orientation_and_hides_without_overflow() {
    use zgx::compose::prelude::*;
    let mut ui = Ui::new(300., 200.);
    let offset = ui.signal(0.);
    let height = ui.signal(400.);
    let read_height = height.clone();
    let mounted = ui.mount(
        scroll(offset.clone())
            .size(200., 80.)
            .p(8.)
            .scrollbar(true)
            .child(div().reactive_style(move || Styles::new().h(read_height.get()))),
    );
    ui.prepare_frame();
    let mut adapter = AccessibilityTree::new();
    let initial = adapter.update(&ui.scene.borrow(), &ui.semantics.borrow(), None, "Bars", 1.);
    let (id, native) = initial
        .nodes
        .iter()
        .find(|(_, n)| n.role() == NativeRole::ScrollBar)
        .unwrap();
    let id = *id;
    assert_eq!(native.orientation(), Some(accesskit::Orientation::Vertical));
    assert_eq!(native.min_numeric_value(), Some(0.));
    assert_eq!(native.max_numeric_value(), Some(336.));
    for action in [
        Action::Focus,
        Action::SetValue,
        Action::Increment,
        Action::Decrement,
    ] {
        assert!(native.supports_action(action));
    }
    let target = adapter.scene_node(id).unwrap();
    let mut consumer = Tree::new(initial, true);
    assert!(ui.set_accessible_numeric_value(target, 50.));
    assert_eq!(offset.get(), 50.);
    assert!(ui.input.focus(&ui.scene, Some(target)));
    let update = adapter.update(
        &ui.scene.borrow(),
        &ui.semantics.borrow(),
        ui.input.focused(),
        "Bars",
        1.,
    );
    consumer.update_and_process_changes(update, &mut Changes::default());
    assert_eq!(
        consumer.state().focus().unwrap().role(),
        NativeRole::ScrollBar
    );
    height.set(40.);
    ui.prepare_frame();
    assert_eq!(offset.get(), 0.);
    assert_ne!(ui.input.focused(), Some(target));
    let update = adapter.update(
        &ui.scene.borrow(),
        &ui.semantics.borrow(),
        ui.input.focused(),
        "Bars",
        1.,
    );
    consumer.update_and_process_changes(update, &mut Changes::default());
    assert_eq!(adapter.scene_node(id), None);
    height.set(400.);
    ui.prepare_frame();
    let update = adapter.update(&ui.scene.borrow(), &ui.semantics.borrow(), None, "Bars", 1.);
    assert!(
        update
            .nodes
            .iter()
            .any(|(_, n)| n.role() == NativeRole::ScrollBar)
    );
    consumer.update_and_process_changes(update, &mut Changes::default());
    ui.set_disabled(mounted.node(), true);
    assert!(!ui.set_accessible_numeric_value(target, 10.));
    assert_eq!(offset.get(), 0.);
}

#[test]
fn virtual_keyboard_navigation_exports_offscreen_row_position_with_bounded_work() {
    use std::{cell::Cell, rc::Rc};
    use zgx::{
        compose::prelude::*,
        input::{Key, Modifiers},
    };
    let mut ui = Ui::new(400., 200.);
    let count = ui.signal(1_000_000usize);
    let read_count = count.clone();
    let keys = Rc::new(Cell::new(0));
    let read_keys = keys.clone();
    let mounted = ui.mount(
        virtual_list(
            ui.signal(0.),
            20.,
            1,
            move || read_count.get(),
            move |i| {
                read_keys.set(read_keys.get() + 1);
                i
            },
            |_, i, _| text(i.to_string()),
        )
        .size(200., 80.)
        .keyboard_navigation(true),
    );
    ui.prepare_frame();
    assert!(ui.input.focus(&ui.scene, Some(mounted.node())));
    let mut adapter = AccessibilityTree::new();
    let initial = adapter.update(
        &ui.scene.borrow(),
        &ui.semantics.borrow(),
        ui.input.focused(),
        "Virtual",
        1.,
    );
    let root = initial
        .nodes
        .iter()
        .find(|(_, n)| n.role() == NativeRole::ScrollView)
        .unwrap();
    assert_eq!(root.1.size_of_set(), Some(1_000_000));
    assert!(root.1.supports_action(Action::Focus));
    let mut consumer = Tree::new(initial, true);
    let before = keys.get();
    ui.dispatch(InputEvent::KeyDown {
        key: Key::End,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    ui.prepare_frame();
    assert!(
        keys.get() - before < 64,
        "End must not scan intervening keys"
    );
    let update = adapter.update(
        &ui.scene.borrow(),
        &ui.semantics.borrow(),
        ui.input.focused(),
        "Virtual",
        1.,
    );
    let row = update
        .nodes
        .iter()
        .find(|(id, _)| *id == update.focus)
        .unwrap();
    assert_eq!(row.1.role(), NativeRole::ListItem);
    assert_eq!(row.1.position_in_set(), Some(1_000_000));
    assert!(row.1.supports_action(Action::Focus));
    consumer.update_and_process_changes(update, &mut Changes::default());
    count.set(3);
    ui.prepare_frame();
    let update = adapter.update(
        &ui.scene.borrow(),
        &ui.semantics.borrow(),
        ui.input.focused(),
        "Virtual",
        1.,
    );
    consumer.update_and_process_changes(update, &mut Changes::default());
    assert_eq!(ui.input.focused(), Some(mounted.node()));
    mounted.unmount();
    let update = adapter.update(
        &ui.scene.borrow(),
        &ui.semantics.borrow(),
        ui.input.focused(),
        "Virtual",
        1.,
    );
    consumer.update_and_process_changes(update, &mut Changes::default());
}
