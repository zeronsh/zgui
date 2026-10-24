use std::{cell::Cell, rc::Rc};
use zgx::{
    compose::prelude::*,
    input::{InputEvent, Key, Modifiers},
    scene::{NodeId, NodeKind},
    semantics::Role,
    widgets::Ui,
};

struct DropCount(Rc<Cell<usize>>);
impl Drop for DropCount {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

fn key(ui: &mut Ui, key: Key) {
    ui.dispatch(InputEvent::KeyDown {
        key,
        modifiers: Modifiers::default(),
        repeat: false,
    });
}

fn position(ui: &Ui) -> Option<usize> {
    ui.input.focused().and_then(|node| {
        ui.semantics
            .borrow()
            .get(node)
            .and_then(|semantic| semantic.position_in_set)
    })
}

fn row(ui: &Ui, child: NodeId) -> NodeId {
    ui.scene.borrow().parent(child).unwrap()
}

#[test]
fn heterogeneous_rows_have_prefix_geometry_and_bounded_scroll_work() {
    let mut ui = Ui::new(400., 300.);
    let offset = ui.signal(0.);
    let heights = VariableHeights::new(&ui.runtime, 100_000, 20.);
    heights.set_height(0, 10.).unwrap();
    heights.set_height(1, 40.).unwrap();
    heights.set_height(2, 30.).unwrap();
    let key_calls = Rc::new(Cell::new(0));
    let calls = key_calls.clone();
    let builds = Rc::new(Cell::new(0));
    let created = builds.clone();
    let view = ui.mount(
        variable_virtual_list(
            offset.clone(),
            heights.clone(),
            1,
            move |index| {
                calls.set(calls.get() + 1);
                index
            },
            move |_, index, _| {
                created.set(created.get() + 1);
                text(index.to_string()).id(format!("row-{index}"))
            },
        )
        .size(200., 100.),
    );
    ui.prepare_frame();
    let second = row(&ui, view.find("row-1").unwrap());
    let bounds = ui.scene.borrow().bounds(second);
    assert_eq!((bounds.y, bounds.height), (10., 40.));
    assert!(builds.get() <= 6);

    // Prefix heights of rows 0..1000 total 20_020 logical pixels.
    offset.set(20_023.);
    ui.prepare_frame();
    let visible = row(&ui, view.find("row-1000").unwrap());
    assert_eq!(ui.scene.borrow().bounds(visible).y, -3.);
    assert!(view.find("row-1").is_none());
    assert!(builds.get() < 20);
    assert!(key_calls.get() < 50, "scroll must not enumerate all keys");

    ui.scene.borrow_mut().flush();
    let before = builds.get();
    offset.set(20_024.);
    ui.prepare_frame();
    let report = ui.scene.borrow_mut().flush();
    assert_eq!(builds.get(), before);
    assert_eq!(report.layout_nodes, 0);
    assert!(report.composite_nodes > 0);
    assert_eq!(heights.content_height(), 2_000_020.);
}

#[test]
fn point_changes_anchor_first_visible_row_not_overscan() {
    let mut ui = Ui::new(200., 100.);
    let offset = ui.signal(45.);
    let heights = VariableHeights::new(&ui.runtime, 20, 20.);
    let view = ui.mount(
        variable_virtual_list(
            offset.clone(),
            heights.clone(),
            2,
            |index| index,
            |_, index, _| text(index.to_string()).id(format!("row-{index}")),
        )
        .size(200., 60.),
    );
    ui.prepare_frame();
    let anchored = view.find("row-2").unwrap();
    let wrapper = row(&ui, anchored);
    assert_eq!(ui.scene.borrow().bounds(wrapper).y, -5.);
    heights.set_height(0, 50.).unwrap();
    ui.prepare_frame();
    assert_eq!(offset.get(), 75.);
    assert_eq!(view.find("row-2"), Some(anchored));
    assert_eq!(ui.scene.borrow().bounds(wrapper).y, -5.);
    heights.set_height(0, 10.).unwrap();
    ui.prepare_frame();
    assert_eq!(offset.get(), 35.);
    assert_eq!(ui.scene.borrow().bounds(wrapper).y, -5.);

    heights.set_height(2, 3.).unwrap();
    ui.prepare_frame();
    assert_eq!(offset.get(), 33_f32.next_down());
    assert_eq!(heights.row_at(offset.get()), Some(2));
    heights.set_height(0, 15.).unwrap();
    ui.prepare_frame();
    assert_eq!(heights.row_at(offset.get()), Some(2));
    assert_eq!(offset.get(), 38_f32.next_down());

    // Explicit pixel requests win over simultaneous anchor compensation,
    // regardless of write order within a reactive batch.
    ui.runtime.batch(|| {
        heights.set_height(0, 50.).unwrap();
        offset.set(10.);
    });
    ui.prepare_frame();
    assert_eq!(offset.get(), 10.);
    ui.runtime.batch(|| {
        offset.set(70.);
        heights.set_height(0, 20.).unwrap();
    });
    ui.prepare_frame();
    assert_eq!(offset.get(), 70.);
}

#[test]
fn retained_keys_keep_local_ownership_and_receive_new_indices() {
    let mut ui = Ui::new(240., 160.);
    let offset = ui.signal(0.);
    let heights = VariableHeights::new(&ui.runtime, 4, 20.);
    heights.set_height(1, 35.).unwrap();
    heights.set_height(2, 25.).unwrap();
    heights.set_height(3, 30.).unwrap();
    let order = ui.signal(vec![10, 11, 12, 13]);
    let read = order.clone();
    let builds = Rc::new(Cell::new(0));
    let created = builds.clone();
    let drops = Rc::new(Cell::new(0));
    let disposed = drops.clone();
    let view = ui.mount(
        variable_virtual_list(
            offset.clone(),
            heights.clone(),
            0,
            move |index| read.with(|keys| keys[index]),
            move |index, item, cx| {
                created.set(created.get() + 1);
                cx.retain(DropCount(disposed.clone()));
                text_signal(move || index.get().to_string()).id(format!("key-{item}"))
            },
        )
        .size(200., 120.)
        .keyboard_navigation(true),
    );
    ui.prepare_frame();
    let retained = view.find("key-10").unwrap();
    let focused_row = row(&ui, retained);
    ui.input.focus(&ui.scene, Some(focused_row));
    ui.runtime.batch(|| {
        order.set(vec![11, 10, 12, 13]);
        // Heights belong to indices: the caller explicitly reorders them.
        heights.set_height(0, 35.).unwrap();
        heights.set_height(1, 20.).unwrap();
    });
    ui.prepare_frame();
    assert_eq!(view.find("key-10"), Some(retained));
    assert_eq!(ui.input.focused(), Some(focused_row));
    assert_eq!(position(&ui), Some(2));
    match ui.scene.borrow().kind(retained) {
        NodeKind::Text { text, .. } => assert_eq!(text.as_ref(), "1"),
        _ => panic!("expected retained text node"),
    }
    assert_eq!(ui.scene.borrow().bounds(focused_row).height, 20.);
    assert_eq!(builds.get(), 4);
    assert_eq!(drops.get(), 0);
    view.unmount();
    assert_eq!(drops.get(), builds.get());
    assert!(!ui.input.has_listeners(focused_row));
    let before = builds.get();
    heights.set_height(0, 99.).unwrap();
    heights.resize(2).unwrap();
    offset.set(1000.);
    order.set(vec![11, 10]);
    assert_eq!(builds.get(), before);
    assert_eq!(
        offset.get(),
        1000.,
        "disposed bindings must not clamp input"
    );
}

#[test]
fn keyboard_pages_use_pixel_prefixes_and_tall_rows_still_make_progress() {
    let mut ui = Ui::new(200., 120.);
    let offset = ui.signal(0.);
    let heights = VariableHeights::new(&ui.runtime, 6, 20.);
    for (index, height) in [10., 70., 20., 50., 15., 35.].into_iter().enumerate() {
        heights.set_height(index, height).unwrap();
    }
    let view = ui.mount(
        variable_virtual_list(
            offset.clone(),
            heights.clone(),
            0,
            |index| index,
            |_, index, _| text(index.to_string()),
        )
        .size(200., 80.)
        .keyboard_navigation(true),
    );
    ui.prepare_frame();
    ui.input.focus(&ui.scene, Some(view.node()));
    key(&mut ui, Key::ArrowDown);
    assert_eq!(position(&ui), Some(1));
    key(&mut ui, Key::PageDown);
    assert_eq!(position(&ui), Some(3));
    assert_eq!(offset.get(), 20.);
    key(&mut ui, Key::PageDown);
    assert_eq!(position(&ui), Some(5));
    key(&mut ui, Key::PageUp);
    assert_eq!(position(&ui), Some(2));
    key(&mut ui, Key::End);
    assert_eq!(position(&ui), Some(6));
    assert_eq!(offset.get(), 120.);
    key(&mut ui, Key::Home);
    assert_eq!(position(&ui), Some(1));
    assert_eq!(offset.get(), 0.);

    heights.set_height(0, 200.).unwrap();
    ui.prepare_frame();
    key(&mut ui, Key::PageDown);
    assert_eq!(
        position(&ui),
        Some(2),
        "oversized rows must not trap PageDown"
    );
    key(&mut ui, Key::PageUp);
    assert_eq!(position(&ui), Some(1));
    assert_eq!(offset.get(), 0.);
}

#[test]
fn scrollbar_tracks_updated_extent_and_resize_clamps_and_hides() {
    let mut ui = Ui::new(240., 120.);
    let offset = ui.signal(0.);
    let heights = VariableHeights::new(&ui.runtime, 4, 20.);
    for (index, height) in [30., 50., 20., 100.].into_iter().enumerate() {
        heights.set_height(index, height).unwrap();
    }
    let view = ui.mount(
        variable_virtual_list(
            offset.clone(),
            heights.clone(),
            0,
            |index| index,
            |_, index, _| text(index.to_string()),
        )
        .size(200., 80.)
        .scrollbar(true),
    );
    ui.prepare_frame();
    let bar = ui
        .semantics
        .borrow()
        .iter()
        .find(|(_, semantic)| semantic.role == Role::ScrollBar)
        .unwrap()
        .0;
    assert_eq!(ui.semantics.borrow().get(bar).unwrap().max, Some(120.));
    ui.input
        .dispatch_to(&ui.scene, bar, InputEvent::SetNumericValue(80.));
    assert_eq!(offset.get(), 80.);
    heights.set_height(0, 50.).unwrap();
    ui.prepare_frame();
    assert_eq!(offset.get(), 100.);
    assert_eq!(ui.semantics.borrow().get(bar).unwrap().max, Some(140.));
    assert_eq!(
        ui.semantics.borrow().get(bar).unwrap().numeric_value,
        Some(100.)
    );
    heights.resize(2).unwrap();
    ui.prepare_frame();
    assert_eq!(offset.get(), 20.);
    ui.input.focus(&ui.scene, Some(bar));
    heights.resize(1).unwrap();
    ui.prepare_frame();
    assert_eq!(offset.get(), 0.);
    assert!(ui.semantics.borrow().get(bar).is_none());
    assert_ne!(ui.input.focused(), Some(bar));
    heights.resize(0).unwrap();
    ui.prepare_frame();
    assert!(heights.is_empty());
    assert_eq!(offset.get(), 0.);
    assert!(view.is_mounted());
}

#[test]
fn failed_row_constructor_rolls_back_partial_rows_and_can_recover() {
    let mut ui = Ui::new(200., 100.);
    let offset = ui.signal(0.);
    let heights = VariableHeights::new(&ui.runtime, 100, 20.);
    let fail = Rc::new(Cell::new(true));
    let should_fail = fail.clone();
    let drops = Rc::new(Cell::new(0));
    let disposed = drops.clone();
    let view = ui.mount(
        variable_virtual_list(
            offset.clone(),
            heights,
            0,
            |index| index,
            move |_, item, cx| {
                assert!(!(item == 3 && should_fail.get()), "row construction failed");
                cx.retain(DropCount(disposed.clone()));
                text(item.to_string()).id(format!("row-{item}"))
            },
        )
        .size(200., 40.),
    );
    ui.prepare_frame();
    let original = view.find("row-0").unwrap();
    let nodes = ui.scene.borrow().len();
    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| offset.set(40.)));
    assert!(failed.is_err());
    assert_eq!(view.find("row-0"), Some(original));
    assert!(view.find("row-2").is_none());
    assert_eq!(ui.scene.borrow().len(), nodes);
    assert_eq!(drops.get(), 1, "partial row ownership must be disposed");
    fail.set(false);
    offset.set(0.);
    ui.dispatch(InputEvent::Scroll {
        x: 10.,
        y: 10.,
        delta_x: 0.,
        delta_y: 20.,
    });
    assert_eq!(offset.get(), 20.);
    assert!(view.find("row-2").is_some());
}

#[test]
fn duplicate_visible_keys_preserve_existing_rows_and_ownership() {
    let mut ui = Ui::new(200., 100.);
    let heights = VariableHeights::new(&ui.runtime, 3, 20.);
    let order = ui.signal(vec![10, 11, 12]);
    let read = order.clone();
    let drops = Rc::new(Cell::new(0));
    let disposed = drops.clone();
    let view = ui.mount(
        variable_virtual_list(
            ui.signal(0.),
            heights,
            0,
            move |index| read.with(|keys| keys[index]),
            move |_, item, cx| {
                cx.retain(DropCount(disposed.clone()));
                text(item.to_string()).id(format!("key-{item}"))
            },
        )
        .size(200., 60.)
        .keyboard_navigation(true),
    );
    ui.prepare_frame();
    let retained = view.find("key-10").unwrap();
    let focused = row(&ui, retained);
    ui.input.focus(&ui.scene, Some(focused));
    let node_count = ui.scene.borrow().len();
    let effect_count = ui.runtime.effect_count();
    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        order.set(vec![10, 10, 12]);
    }));
    assert!(failed.is_err());
    assert_eq!(view.find("key-10"), Some(retained));
    assert!(view.find("key-11").is_some());
    assert_eq!(ui.input.focused(), Some(focused));
    assert_eq!(ui.scene.borrow().len(), node_count);
    assert_eq!(ui.runtime.effect_count(), effect_count);
    assert_eq!(drops.get(), 0);
    order.set(vec![11, 10, 12]);
    ui.prepare_frame();
    assert_eq!(view.find("key-10"), Some(retained));
    assert_eq!(position(&ui), Some(2));
    view.unmount();
    assert_eq!(drops.get(), 3);
}

#[test]
fn interactive_descendants_keep_editor_and_button_keyboard_behavior() {
    let mut ui = Ui::new(240., 180.);
    let offset = ui.signal(0.);
    let heights = VariableHeights::new(&ui.runtime, 100, 25.);
    heights.set_height(0, 40.).unwrap();
    heights.set_height(1, 55.).unwrap();
    let value = ui.signal("abc".to_string());
    let editor = value.clone();
    let calls = ui.signal(0);
    let clicked = calls.clone();
    let view = ui.mount(
        variable_virtual_list(
            offset.clone(),
            heights.clone(),
            0,
            |index| index,
            move |_, index, _| match index {
                0 => text_input("Editor", editor.clone())
                    .size(180., 40.)
                    .id("editor"),
                1 => {
                    let clicked = clicked.clone();
                    button()
                        .size(180., 55.)
                        .id("button")
                        .child(text("Run"))
                        .on_click(move || {
                            clicked.set(clicked.get() + 1);
                        })
                }
                _ => text(index.to_string()),
            },
        )
        .size(200., 130.)
        .keyboard_navigation(true),
    );
    ui.prepare_frame();
    ui.input.focus(&ui.scene, view.find("editor"));
    key(&mut ui, Key::Home);
    ui.dispatch(InputEvent::Text("x".into()));
    assert_eq!(value.get(), "xabc");
    for input in [Key::ArrowDown, Key::PageDown, Key::End] {
        key(&mut ui, input);
        assert_eq!(ui.input.focused(), view.find("editor"));
        assert_eq!(offset.get(), 0.);
    }
    ui.input.focus(&ui.scene, view.find("button"));
    for input in [Key::Home, Key::End, Key::PageDown] {
        key(&mut ui, input);
        assert_eq!(ui.input.focused(), view.find("button"));
        assert_eq!(offset.get(), 0.);
    }
    key(&mut ui, Key::Enter);
    ui.dispatch(InputEvent::KeyUp {
        key: Key::Enter,
        modifiers: Modifiers::default(),
    });
    assert_eq!(calls.get(), 1);
    assert_eq!(ui.input.focused(), view.find("button"));
    heights.set_height(0, 60.).unwrap();
    ui.prepare_frame();
    assert_eq!(ui.input.focused(), view.find("button"));
    assert_eq!(offset.get(), 0.);
}

#[test]
fn empty_model_can_grow_and_empty_again_without_leaking_row_subscriptions() {
    let mut ui = Ui::new(200., 100.);
    let offset = ui.signal(0.);
    let heights = VariableHeights::new(&ui.runtime, 0, 20.);
    let builds = Rc::new(Cell::new(0));
    let created = builds.clone();
    let drops = Rc::new(Cell::new(0));
    let disposed = drops.clone();
    let effects_before_mount = ui.runtime.effect_count();
    let view = ui.mount(
        variable_virtual_list(
            offset.clone(),
            heights.clone(),
            0,
            |index| index,
            move |index, item, cx| {
                created.set(created.get() + 1);
                cx.retain(DropCount(disposed.clone()));
                text_signal(move || index.get().to_string()).id(format!("row-{item}"))
            },
        )
        .size(200., 60.)
        .keyboard_navigation(true),
    );
    ui.prepare_frame();
    ui.input.focus(&ui.scene, Some(view.node()));
    key(&mut ui, Key::End);
    assert_eq!(ui.input.focused(), Some(view.node()));
    assert_eq!(builds.get(), 0);
    assert_eq!(
        ui.semantics.borrow().get(view.node()).unwrap().size_of_set,
        Some(0)
    );
    let empty_effects = ui.runtime.effect_count();
    heights.resize(3).unwrap();
    ui.prepare_frame();
    assert_eq!(builds.get(), 3);
    key(&mut ui, Key::ArrowDown);
    let removed = ui.input.focused().unwrap();
    assert_eq!(position(&ui), Some(1));
    heights.resize(0).unwrap();
    ui.prepare_frame();
    assert_eq!(drops.get(), 3);
    assert_eq!(ui.input.focused(), Some(view.node()));
    assert!(!ui.input.has_listeners(removed));
    assert_eq!(ui.runtime.effect_count(), empty_effects);
    assert_eq!(
        ui.semantics.borrow().get(view.node()).unwrap().size_of_set,
        Some(0)
    );
    heights.resize(2).unwrap();
    ui.prepare_frame();
    assert_eq!(builds.get(), 5);
    assert_eq!(heights.row_height(0), Some(20.));
    view.unmount();
    assert_eq!(drops.get(), builds.get());
    assert_eq!(ui.runtime.effect_count(), effects_before_mount);
    heights.set_height(0, 90.).unwrap();
    heights.resize(0).unwrap();
    heights.resize(10).unwrap();
    offset.set(999.);
    assert_eq!(offset.get(), 999.);
    assert_eq!(builds.get(), 5);
}

#[test]
fn invalid_and_equal_height_writes_do_not_notify_reactive_consumers() {
    let ui = Ui::new(200., 100.);
    let heights = VariableHeights::new(&ui.runtime, 3, 20.);
    let observed = heights.clone();
    let notifications = Rc::new(Cell::new(0));
    let count = notifications.clone();
    let effect = ui.runtime.effect(move || {
        let _ = (
            observed.len(),
            observed.is_empty(),
            observed.row_height(0),
            observed.row_offset(1),
            observed.content_height(),
            observed.row_at(0.),
        );
        count.set(count.get() + 1);
    });
    assert_eq!(notifications.get(), 1);
    assert!(!heights.set_height(1, 20.).unwrap());
    assert!(!heights.resize(3).unwrap());
    for invalid in [0., -1., f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert!(heights.set_height(1, invalid).is_err());
    }
    assert!(heights.set_height(3, 20.).is_err());
    assert!(heights.resize(usize::MAX).is_err());
    assert_eq!(notifications.get(), 1);
    assert_eq!(heights.len(), 3);
    assert_eq!(heights.row_height(1), Some(20.));
    assert_eq!(heights.content_height(), 60.);
    assert!(heights.set_height(1, 30.).unwrap());
    assert_eq!(notifications.get(), 2);
    ui.runtime.batch(|| {
        heights.set_height(0, 40.).unwrap();
        heights.resize(4).unwrap();
    });
    assert_eq!(notifications.get(), 3);
    drop(effect);
    heights.resize(0).unwrap();
    assert_eq!(notifications.get(), 3);
}

#[test]
fn explicit_scroll_request_from_key_callback_is_not_overwritten() {
    let mut ui = Ui::new(200., 100.);
    let offset = ui.signal(0.);
    let request = offset.clone();
    let sent = Rc::new(Cell::new(false));
    let write_once = sent.clone();
    let heights = VariableHeights::new(&ui.runtime, 100, 20.);
    let view = ui.mount(
        variable_virtual_list(
            offset.clone(),
            heights,
            0,
            move |index| {
                if !write_once.replace(true) {
                    request.set(200.);
                }
                index
            },
            |_, index, _| text(index.to_string()).id(format!("row-{index}")),
        )
        .size(200., 40.),
    );
    ui.prepare_frame();
    assert!(sent.get());
    assert_eq!(offset.get(), 200.);
    assert!(view.find("row-10").is_some());
    assert!(view.find("row-0").is_none());
}
