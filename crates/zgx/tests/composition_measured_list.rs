use std::{cell::Cell, rc::Rc};
use zgx::{
    compose::prelude::*,
    input::{InputEvent, Key, Modifiers},
    scene::{NodeId, NodeKind},
    widgets::Ui,
};

struct DropCount(Rc<Cell<usize>>);
impl Drop for DropCount {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

fn row(ui: &Ui, child: NodeId) -> NodeId {
    ui.scene.borrow().parent(child).unwrap()
}

#[test]
fn huge_estimates_discover_tiny_rows_within_the_layout_feedback_budget() {
    let mut ui = Ui::new(240., 340.);
    let heights = VariableHeights::new(&ui.runtime, 100_000, 1_000_000.);
    let builds = Rc::new(Cell::new(0));
    let created = builds.clone();
    let drops = Rc::new(Cell::new(0));
    let disposed = drops.clone();
    let view = ui.mount(
        measured_virtual_list(
            ui.signal(0.),
            heights.clone(),
            1,
            |index| index,
            move |_, index, cx| {
                created.set(created.get() + 1);
                assert!(
                    created.get() <= 2048,
                    "discovery mounted an unbounded range"
                );
                cx.retain(DropCount(disposed.clone()));
                div().h(1.).id(format!("row-{index}"))
            },
        )
        .size(200., 300.),
    );
    // The naive one-new-row-per-layout approach exceeds the shared 64-pass cap.
    ui.try_prepare_frame().unwrap();
    assert!(builds.get() <= 1024);
    assert!(builds.get() - drops.get() <= 302);
    let bottom = row(&ui, view.find("row-299").unwrap());
    assert_eq!(ui.scene.borrow().bounds(bottom).y, 299.);
    assert_eq!(ui.scene.borrow().bounds(bottom).height, 1.);
    assert_eq!(heights.row_height(299), Some(1.));
    let settled = builds.get();
    ui.scene.borrow_mut().flush();
    ui.try_prepare_frame().unwrap();
    assert_eq!(builds.get(), settled);
    assert_eq!(ui.scene.borrow_mut().flush().layout_nodes, 0);
    view.unmount();
    assert_eq!(builds.get(), drops.get());
}

#[test]
fn zero_height_children_use_minimum_allocation_even_with_focus_markers() {
    let mut ui = Ui::new(200., 80.);
    let heights = VariableHeights::new(&ui.runtime, 1000, 1000.);
    let builds = Rc::new(Cell::new(0));
    let created = builds.clone();
    let view = ui.mount(
        measured_virtual_list(
            ui.signal(0.),
            heights.clone(),
            0,
            |index| index,
            move |_, index, _| {
                created.set(created.get() + 1);
                assert!(
                    created.get() < 100,
                    "zero-height discovery must stay bounded"
                );
                div().h(0.).id(format!("row-{index}"))
            },
        )
        .size(100., 10.)
        .keyboard_navigation(true),
    );
    ui.try_prepare_frame().unwrap();
    assert!(builds.get() <= 32);
    for index in 0..10 {
        let wrapper = row(&ui, view.find(&format!("row-{index}")).unwrap());
        assert_eq!(heights.row_height(index), Some(1.));
        let bounds = ui.scene.borrow().bounds(wrapper);
        assert_eq!((bounds.y, bounds.height), (index as f32, 1.));
    }
}

#[test]
fn natural_height_changes_include_padding_and_margins_and_preserve_anchor() {
    let mut ui = Ui::new(240., 140.);
    let offset = ui.signal(51.);
    let heights = VariableHeights::new(&ui.runtime, 20, 20.);
    heights.set_height(0, 26.).unwrap();
    let content_height = ui.signal(16.);
    let read = content_height.clone();
    let view = ui.mount(
        measured_virtual_list(
            offset.clone(),
            heights.clone(),
            2,
            |index| index,
            move |_, index, _| {
                if index == 0 {
                    let read = read.clone();
                    column()
                        .p(2.)
                        .m(3.)
                        .id("row-0")
                        .child(div().reactive_style(move || Styles::new().h(read.get())))
                } else {
                    div().h(20.).id(format!("row-{index}"))
                }
            },
        )
        .size(200., 60.),
    );
    ui.try_prepare_frame().unwrap();
    let anchor = view.find("row-2").unwrap();
    let wrapper = row(&ui, anchor);
    assert_eq!(heights.row_height(0), Some(26.));
    assert_eq!(offset.get(), 51.);
    assert_eq!(ui.scene.borrow().bounds(wrapper).y, -5.);
    content_height.set(40.);
    ui.try_prepare_frame().unwrap();
    assert_eq!(heights.row_height(0), Some(50.));
    assert_eq!(offset.get(), 75.);
    assert_eq!(view.find("row-2"), Some(anchor));
    assert_eq!(ui.scene.borrow().bounds(wrapper).y, -5.);
    ui.scene.borrow_mut().flush();
    ui.try_prepare_frame().unwrap();
    assert_eq!(ui.scene.borrow_mut().flush().layout_nodes, 0);
}

#[test]
fn rows_measured_above_the_view_grow_above_what_is_in_view() {
    let mut ui = Ui::new(240., 140.);
    // Fifty-point rows, estimated at twenty: the first fifty rows' estimates
    // reach 1000, where row 50 starts.
    let offset = ui.signal(1000.);
    let heights = VariableHeights::new(&ui.runtime, 100, 20.);
    let view = ui.mount(
        measured_virtual_list(
            offset.clone(),
            heights.clone(),
            0,
            |index| index,
            |_, index, _| div().h(50.).id(format!("row-{index}")),
        )
        .size(200., 100.),
    );
    ui.try_prepare_frame().unwrap();
    let shown = row(&ui, view.find("row-50").unwrap());
    assert_eq!(ui.scene.borrow().bounds(shown).y, 0.);
    // Up by 30: rows 48 and 49 come into view as estimates and measure 50
    // each; row 50 moves down by the 30 alone.
    offset.set(970.);
    ui.try_prepare_frame().unwrap();
    assert_eq!(heights.row_height(49), Some(50.));
    assert_eq!(ui.scene.borrow().bounds(shown).y, 30.);
    assert_eq!(offset.get(), 1030.);
    ui.scene.borrow_mut().flush();
    ui.try_prepare_frame().unwrap();
    assert_eq!(ui.scene.borrow().bounds(shown).y, 30.);
}

#[test]
fn wrapped_text_remeasures_visible_width_and_offscreen_rows_on_remount() {
    let mut ui = Ui::new(240., 140.);
    ui.scene
        .borrow_mut()
        .set_text_measurer(|text: &str, _: f32, maximum: Option<f32>| {
            let natural = text.chars().count() as f32 * 10.;
            let width = maximum.unwrap_or(natural).max(1.);
            (natural.min(width), (natural / width).ceil().max(1.) * 10.)
        });
    let offset = ui.signal(0.);
    let heights = VariableHeights::new(&ui.runtime, 10, 20.);
    let width = ui.signal(100.);
    let read = width.clone();
    let view = ui.mount(
        measured_virtual_list(
            offset.clone(),
            heights.clone(),
            0,
            |index| index,
            |_, index, _| {
                text("abcdefghijklmnopqrst")
                    .text_wrap(true)
                    .id(format!("row-{index}"))
            },
        )
        .h(50.)
        .reactive_style(move || Styles::new().w(read.get())),
    );
    ui.try_prepare_frame().unwrap();
    let first = view.find("row-0").unwrap();
    assert_eq!(heights.row_height(0), Some(20.));
    assert_eq!(ui.scene.borrow().bounds(first).height, 20.);
    assert!(view.find("row-9").is_none());
    width.set(50.);
    ui.try_prepare_frame().unwrap();
    assert_eq!(view.find("row-0"), Some(first));
    assert_eq!(heights.row_height(0), Some(40.));
    assert_eq!(ui.scene.borrow().bounds(first).height, 40.);
    assert_eq!(
        heights.row_height(9),
        Some(20.),
        "offscreen allocation remains an estimate"
    );
    offset.set(heights.row_offset(9));
    ui.try_prepare_frame().unwrap();
    assert_eq!(heights.row_height(9), Some(40.));
    // Growing the newly measured rows preserves the first visible anchor;
    // use the corrected prefix to bring the last row back into the viewport.
    offset.set(heights.row_offset(9));
    ui.try_prepare_frame().unwrap();
    let last = view.find("row-9").unwrap();
    assert_eq!(ui.scene.borrow().bounds(last).height, 40.);
}

#[test]
fn retained_keys_and_measurement_subscriptions_follow_component_lifetimes() {
    let mut ui = Ui::new(240., 160.);
    let offset = ui.signal(0.);
    let heights = VariableHeights::new(&ui.runtime, 3, 20.);
    let items = ui.signal(vec![10, 11, 12]);
    let read = items.clone();
    let drops = Rc::new(Cell::new(0));
    let disposed = drops.clone();
    let builds = Rc::new(Cell::new(0));
    let created = builds.clone();
    let before = ui.runtime.effect_count();
    let view = ui.mount(
        measured_virtual_list(
            offset.clone(),
            heights.clone(),
            0,
            move |index| read.with(|items| items[index]),
            move |index, item, cx| {
                created.set(created.get() + 1);
                cx.retain(DropCount(disposed.clone()));
                let height = match item {
                    10 => 10.,
                    11 => 30.,
                    _ => 20.,
                };
                column()
                    .h(height)
                    .id(format!("row-{item}"))
                    .child(text_signal(move || index.get().to_string()).id(format!("index-{item}")))
            },
        )
        .size(200., 100.),
    );
    ui.try_prepare_frame().unwrap();
    let retained = view.find("row-10").unwrap();
    assert_eq!(heights.row_height(0), Some(10.));
    assert_eq!(heights.row_height(1), Some(30.));
    ui.runtime.batch(|| {
        items.set(vec![11, 10, 12]);
        heights.set_height(0, 30.).unwrap();
        heights.set_height(1, 10.).unwrap();
    });
    ui.try_prepare_frame().unwrap();
    assert_eq!(view.find("row-10"), Some(retained));
    match ui.scene.borrow().kind(view.find("index-10").unwrap()) {
        NodeKind::Text { text, .. } => assert_eq!(text.as_ref(), "1"),
        _ => panic!("expected retained index text"),
    }
    assert_eq!(heights.row_height(1), Some(10.));
    assert_eq!(builds.get(), 3);
    assert_eq!(drops.get(), 0);
    view.unmount();
    assert_eq!(drops.get(), 3);
    assert_eq!(ui.runtime.effect_count(), before);
    heights.set_height(0, 90.).unwrap();
    items.set(vec![10, 11, 12]);
    offset.set(999.);
    ui.try_prepare_frame().unwrap();
    assert_eq!(offset.get(), 999.);
    assert_eq!(builds.get(), 3);
}

#[test]
fn true_measurement_feedback_is_reported_and_can_be_repaired() {
    let mut ui = Ui::new(200., 120.);
    let heights = VariableHeights::new(&ui.runtime, 1, 20.);
    let enabled = ui.signal(false);
    let active = enabled.clone();
    let measured = heights.clone();
    let updates = Rc::new(Cell::new(0));
    let count = updates.clone();
    let delivered = Rc::new(Cell::new(0));
    let events = delivered.clone();
    ui.on_event(ui.root(), false, move |_| events.set(events.get() + 1));
    let view = ui.mount(
        measured_virtual_list(
            ui.signal(0.),
            heights.clone(),
            0,
            |index| index,
            move |_, _, _| {
                let active = active.clone();
                let measured = measured.clone();
                let count = count.clone();
                div().id("row").reactive_style(move || {
                    let height = if active.get() {
                        count.set(count.get() + 1);
                        assert!(count.get() < 300, "measurement feedback did not stop");
                        if measured.row_height(0) == Some(20.) {
                            30.
                        } else {
                            20.
                        }
                    } else {
                        20.
                    };
                    Styles::new().h(height)
                })
            },
        )
        .size(200., 80.),
    );
    ui.try_prepare_frame().unwrap();
    enabled.set(true);
    assert!(ui.try_prepare_frame().is_err());
    assert!(updates.get() > 1);
    assert!(
        ui.try_dispatch(InputEvent::PointerMove { x: 1., y: 1. })
            .is_err()
    );
    assert_eq!(
        delivered.get(),
        0,
        "unstable geometry must not receive input"
    );
    enabled.set(false);
    ui.try_prepare_frame().unwrap();
    assert_eq!(heights.row_height(0), Some(20.));
    assert_eq!(
        ui.scene.borrow().bounds(view.find("row").unwrap()).height,
        20.
    );
    let settled = updates.get();
    ui.try_prepare_frame().unwrap();
    assert_eq!(updates.get(), settled);
    ui.try_dispatch(InputEvent::PointerMove { x: 1., y: 1. })
        .unwrap();
    assert!(delivered.get() > 0);
}

#[test]
fn keyboard_reveal_survives_measurement_growth_before_the_target() {
    let mut ui = Ui::new(240., 140.);
    let offset = ui.signal(0.);
    let heights = VariableHeights::new(&ui.runtime, 30, 20.);
    let view = ui.mount(
        measured_virtual_list(
            offset.clone(),
            heights.clone(),
            0,
            |index| index,
            |_, index, _| {
                div()
                    .h(if index == 5 { 500. } else { 20. })
                    .id(format!("row-{index}"))
            },
        )
        .size(200., 100.)
        .keyboard_navigation(true),
    );
    ui.try_prepare_frame().unwrap();
    assert!(view.find("row-5").is_none());
    let fourth = row(&ui, view.find("row-4").unwrap());
    assert!(ui.input.focus(&ui.scene, Some(fourth)));
    ui.dispatch(InputEvent::KeyDown {
        key: Key::PageDown,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    ui.try_prepare_frame().unwrap();
    assert_eq!(heights.row_height(5), Some(500.));
    let focused = ui.input.focused().expect("keyboard target retains focus");
    assert_eq!(
        ui.semantics
            .borrow()
            .get(focused)
            .and_then(|node| node.position_in_set),
        Some(10),
        "measurement of preceding rows must not discard the pending target",
    );
    let target = row(&ui, view.find("row-9").expect("target must remain mounted"));
    assert_eq!(focused, target);
    let viewport = ui.scene.borrow().bounds(view.node());
    let bounds = ui.scene.borrow().bounds(target);
    assert!(bounds.y >= viewport.y && bounds.y + bounds.height <= viewport.y + viewport.height);
    let settled_offset = offset.get();
    ui.try_prepare_frame().unwrap();
    assert_eq!(ui.input.focused(), Some(target));
    assert_eq!(offset.get(), settled_offset);
}

#[test]
fn tiny_estimates_and_later_updates_respect_minimum_allocation() {
    let mut ui = Ui::new(240., 140.);
    let offset = ui.signal(0.);
    let heights = VariableHeights::new(&ui.runtime, 100_000, 0.0001);
    let builds = Rc::new(Cell::new(0));
    let created = builds.clone();
    let view = ui.mount(
        measured_virtual_list(
            offset.clone(),
            heights.clone(),
            0,
            |index| index,
            move |_, index, _| {
                created.set(created.get() + 1);
                assert!(created.get() < 1000, "tiny estimates mounted too many rows");
                div().h(1.).id(format!("row-{index}"))
            },
        )
        .size(200., 100.)
        .keyboard_navigation(true),
    );
    ui.try_prepare_frame().unwrap();
    assert!(builds.get() <= 101);
    assert_eq!(heights.row_height(50_000), Some(1.));
    assert!(!heights.set_height(50_000, 0.01).unwrap());
    assert_eq!(heights.row_height(50_000), Some(1.));
    heights.resize(100_005).unwrap();
    assert_eq!(heights.row_height(100_004), Some(1.));
    assert_eq!(heights.content_height(), 100_005.);
    ui.input.focus(&ui.scene, Some(view.node()));
    ui.dispatch(InputEvent::KeyDown {
        key: Key::End,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    ui.try_prepare_frame().unwrap();
    assert!(builds.get() <= 202);
    let last = row(&ui, view.find("row-100004").unwrap());
    assert_eq!(ui.input.focused(), Some(last));
    assert_eq!(ui.scene.borrow().bounds(last).y, 99.);
    assert_eq!(offset.get(), 99_905.);
}

#[test]
fn failed_measured_constructor_discards_partial_observers_and_recovers() {
    let mut ui = Ui::new(200., 100.);
    let offset = ui.signal(0.);
    let heights = VariableHeights::new(&ui.runtime, 10, 20.);
    let fail = Rc::new(Cell::new(true));
    let should_fail = fail.clone();
    let drops = Rc::new(Cell::new(0));
    let disposed = drops.clone();
    let effects_before_mount = ui.runtime.effect_count();
    let view = ui.mount(
        measured_virtual_list(
            offset.clone(),
            heights.clone(),
            0,
            |index| index,
            move |_, index, cx| {
                assert!(!(index == 3 && should_fail.get()), "constructor failed");
                cx.retain(DropCount(disposed.clone()));
                div()
                    .h(if index == 2 { 100. } else { 20. })
                    .id(format!("row-{index}"))
            },
        )
        .size(200., 40.),
    );
    ui.try_prepare_frame().unwrap();
    let original = view.find("row-0").unwrap();
    let nodes = ui.scene.borrow().len();
    let effects = ui.runtime.effect_count();
    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| offset.set(40.)));
    assert!(failed.is_err());
    assert_eq!(view.find("row-0"), Some(original));
    assert!(view.find("row-2").is_none());
    assert_eq!(
        heights.row_height(2),
        Some(20.),
        "failed row must not publish its measurement"
    );
    assert_eq!(ui.scene.borrow().len(), nodes);
    assert_eq!(ui.runtime.effect_count(), effects);
    assert_eq!(drops.get(), 1);
    fail.set(false);
    offset.set(0.);
    offset.set(40.);
    ui.try_prepare_frame().unwrap();
    assert_eq!(heights.row_height(2), Some(100.));
    assert_eq!(
        ui.scene.borrow().bounds(view.find("row-2").unwrap()).height,
        100.
    );
    view.unmount();
    assert_eq!(ui.runtime.effect_count(), effects_before_mount);
    heights.set_height(2, 25.).unwrap();
    ui.try_prepare_frame().unwrap();
    assert_eq!(heights.row_height(2), Some(25.));
}

#[test]
fn measurement_key_callback_can_request_scroll_and_mutate_height_cache() {
    let mut ui = Ui::new(240., 140.);
    let offset = ui.signal(0.);
    let requested = offset.clone();
    let heights = VariableHeights::new(&ui.runtime, 100, 20.);
    let changed = heights.clone();
    let natural = ui.signal(20.);
    let read = natural.clone();
    let armed = Rc::new(Cell::new(false));
    let write_once = armed.clone();
    let view = ui.mount(
        measured_virtual_list(
            offset.clone(),
            heights.clone(),
            0,
            move |index| {
                if index == 0 && write_once.replace(false) {
                    requested.set(200.);
                    changed.set_height(50, 25.).unwrap();
                }
                index
            },
            move |_, index, _| {
                if index == 0 {
                    let read = read.clone();
                    div()
                        .reactive_style(move || Styles::new().h(read.get()))
                        .id("row-0")
                } else {
                    div().h(20.).id(format!("row-{index}"))
                }
            },
        )
        .size(200., 60.),
    );
    ui.try_prepare_frame().unwrap();
    armed.set(true);
    natural.set(30.);
    ui.try_prepare_frame().unwrap();
    assert!(!armed.get());
    assert_eq!(offset.get(), 200.);
    assert_eq!(heights.row_height(0), Some(30.));
    assert_eq!(heights.row_height(50), Some(25.));
    assert!(view.find("row-0").is_none());
    assert!(view.find("row-9").is_some());
}

#[test]
fn explicit_scroll_cancels_a_keyboard_reveal_waiting_for_measurement() {
    let mut ui = Ui::new(240., 140.);
    let offset = ui.signal(0.);
    let heights = VariableHeights::new(&ui.runtime, 30, 20.);
    let view = ui.mount(
        measured_virtual_list(
            offset.clone(),
            heights,
            0,
            |index| index,
            |_, index, _| {
                div()
                    .h(if index == 5 { 500. } else { 20. })
                    .id(format!("row-{index}"))
            },
        )
        .size(200., 100.)
        .keyboard_navigation(true),
    );
    ui.try_prepare_frame().unwrap();
    let focused = row(&ui, view.find("row-4").unwrap());
    ui.input.focus(&ui.scene, Some(focused));
    ui.dispatch(InputEvent::KeyDown {
        key: Key::PageDown,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    // Dispatch queues construction; these new observations settle on the next
    // frame. The caller's newer pixel request supersedes that keyboard reveal.
    offset.set(0.);
    ui.try_prepare_frame().unwrap();
    assert_eq!(offset.get(), 0.);
    assert!(view.find("row-0").is_some());
    assert!(view.find("row-9").is_none());
}

#[test]
fn pending_keyboard_measurement_does_not_steal_new_programmatic_focus() {
    let mut ui = Ui::new(240., 180.);
    let heights = VariableHeights::new(&ui.runtime, 30, 20.);
    let view = ui.mount(
        column()
            .child(
                measured_virtual_list(
                    ui.signal(0.),
                    heights,
                    0,
                    |index| index,
                    |_, index, _| {
                        div()
                            .h(if index == 5 { 500. } else { 20. })
                            .id(format!("row-{index}"))
                    },
                )
                .size(200., 100.)
                .keyboard_navigation(true),
            )
            .child(
                button()
                    .size(200., 30.)
                    .id("outside")
                    .child(text("Outside")),
            ),
    );
    ui.try_prepare_frame().unwrap();
    let focused = row(&ui, view.find("row-4").unwrap());
    assert!(ui.input.focus(&ui.scene, Some(focused)));
    ui.dispatch(InputEvent::KeyDown {
        key: Key::PageDown,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    let outside = view.find("outside").unwrap();
    assert!(ui.input.focus(&ui.scene, Some(outside)));
    ui.try_prepare_frame().unwrap();
    assert_eq!(
        ui.input.focused(),
        Some(outside),
        "newer programmatic focus supersedes pending keyboard navigation"
    );
}

#[test]
fn redirected_focus_during_reveal_is_not_retried_after_measurement() {
    let mut ui = Ui::new(240., 180.);
    let heights = VariableHeights::new(&ui.runtime, 30, 20.);
    let view = ui.mount(
        column()
            .child(
                measured_virtual_list(
                    ui.signal(0.),
                    heights,
                    0,
                    |i| i,
                    |_, i, _| {
                        div()
                            .h(if i == 5 { 500. } else { 20. })
                            .id(format!("row-{i}"))
                    },
                )
                .id("list")
                .size(200., 100.)
                .keyboard_navigation(true),
            )
            .child(
                button()
                    .id("outside")
                    .size(200., 30.)
                    .child(text("Outside")),
            ),
    );
    ui.prepare_frame();
    ui.input
        .focus(&ui.scene, Some(row(&ui, view.find("row-4").unwrap())));
    let outside = view.find("outside").unwrap();
    let scene = ui.scene.clone();
    let input = ui.input.clone();
    let mut armed = true;
    ui.on_event(view.find("list").unwrap(), true, move |cx| {
        if matches!(cx.event, InputEvent::Focus) && armed {
            armed = false;
            input.focus(&scene, Some(outside));
        }
    });
    ui.dispatch(InputEvent::KeyDown {
        key: Key::PageDown,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    ui.try_prepare_frame().unwrap();
    assert_eq!(ui.input.focused(), Some(outside));
}

#[test]
fn end_anchored_lists_stay_at_the_end_through_measurement_and_growth() {
    let mut ui = Ui::new(200., 300.);
    // Estimates far above the real 20 px rows, as for unmeasured messages.
    let heights = VariableHeights::new(&ui.runtime, 60, 90.).anchor_end();
    let offset = ui.signal(1e9_f32);
    let last = ui.signal(20_f32);
    let tall = last.clone();
    let _view = ui.mount(
        measured_virtual_list(
            offset.clone(),
            heights.clone(),
            1,
            |index| index,
            move |_, index, _| {
                let tall = tall.clone();
                div().reactive_style(move || {
                    Styles::new().h(if index == 59 { tall.get() } else { 20. })
                })
            },
        )
        .size(200., 200.),
    );
    let at_end = |ui: &mut Ui| {
        ui.try_prepare_frame().unwrap();
        ui.try_prepare_frame().unwrap();
        (offset.get(), heights.content_height() - 200.)
    };
    let (position, end) = at_end(&mut ui);
    assert_eq!(position, end);
    // Rows measured on the way up keep the viewport at the end.
    last.set(140.);
    let (position, end) = at_end(&mut ui);
    assert_eq!(heights.row_height(59), Some(140.));
    assert_eq!(position, end);
    heights.resize(61).unwrap();
    let (position, end) = at_end(&mut ui);
    assert_eq!(position, end);
    // Scrolled away from the end, the first visible row anchors as before.
    offset.set(end - 100.);
    ui.try_prepare_frame().unwrap();
    let first = heights.row_at(offset.get()).unwrap();
    let top = heights.row_offset(first) - offset.get();
    last.set(400.);
    ui.try_prepare_frame().unwrap();
    ui.try_prepare_frame().unwrap();
    assert_eq!(heights.row_offset(first) - offset.get(), top);
    assert!(offset.get() < heights.content_height() - 200.);
}

#[test]
fn measured_rows_keep_heights_by_key_and_estimate_the_unmounted() {
    let mut ui = Ui::new(200., 300.);
    let heights = VariableHeights::new(&ui.runtime, 0, 50.);
    let keys = ui.signal((0..100_u32).collect::<Vec<_>>());
    let offset = ui.signal(0_f32);
    let estimates = Rc::new(Cell::new(0));
    let counted = estimates.clone();
    let rows = keys.clone();
    ui.mount(
        measured_rows(
            offset.clone(),
            heights.clone(),
            1,
            move || rows.get(),
            // What an app knows before mounting: key k is 20 + k % 5 px tall.
            move |keys: &[u32], index| {
                counted.set(counted.get() + 1);
                20. + (keys[index] % 5) as f32
            },
            // Mounted, a row is 2 px taller than estimated.
            |_, key, _| div().h(22. + (key % 5) as f32),
        )
        .size(200., 200.),
    );
    ui.prepare_frame();
    ui.prepare_frame();
    let height_of = |key: u32| {
        let index = keys.get().iter().position(|k| *k == key).unwrap();
        heights.row_height(index).unwrap()
    };
    // Mounted rows are measured; the rest carry their estimates.
    assert_eq!(height_of(0), 22.);
    assert_eq!(height_of(99), 24.);
    assert_eq!(estimates.get(), 100);
    // Inserting and reordering keeps every known height by key and
    // estimates only the new keys.
    keys.update(|keys| {
        keys.reverse();
        keys.splice(0..0, [1000, 1001]);
    });
    ui.prepare_frame();
    ui.prepare_frame();
    assert_eq!(estimates.get(), 102);
    assert_eq!(heights.len(), 102);
    // Measured at the top, now offscreen at the end: still measured.
    assert_eq!(height_of(0), 22.);
    // Still unmounted: still the estimate.
    assert_eq!(height_of(50), 20.);
    // Now at the top: mounted and measured.
    assert_eq!(height_of(1001), 23.);
    assert_eq!(height_of(99), 26.);
}

#[test]
fn a_far_measured_row_scrolled_to_lands_where_asked() {
    use std::cell::RefCell;
    use zgx::compose::{NodeHandle, RowAlign};
    let mut ui = Ui::new(240., 340.);
    // Rows of seven heights, estimated at twenty.
    let heights = VariableHeights::new(&ui.runtime, 1000, 20.);
    let tall = |index: usize| 10. + (index % 7) as f32 * 9.;
    let handle: Rc<RefCell<Option<NodeHandle>>> = Rc::default();
    let slot = handle.clone();
    let view = ui.mount(
        measured_virtual_list(
            ui.signal(0.),
            heights.clone(),
            2,
            |index| index,
            move |_, index, _| div().h(tall(index)).id(format!("row-{index}")),
        )
        .size(200., 300.)
        .on_mount(move |list| *slot.borrow_mut() = Some(list)),
    );
    ui.try_prepare_frame().unwrap();
    let list = handle.borrow().clone().unwrap();
    let top = ui.scene.borrow().bounds(list.node()).y;
    let at = |ui: &Ui, index: usize| {
        let node = row(ui, view.find(&format!("row-{index}")).unwrap());
        ui.scene.borrow().bounds(node)
    };
    // From the top to row 700: everything above it was an estimate.
    assert!(list.scroll_to_row(700, RowAlign::Start));
    ui.try_prepare_frame().unwrap();
    assert_eq!(at(&ui, 700).y, top, "its top at the top");
    assert!(list.scroll_to_row(300, RowAlign::End));
    ui.try_prepare_frame().unwrap();
    let b = at(&ui, 300);
    assert_eq!(b.y + b.height, top + 300., "its bottom at the bottom");
    assert_eq!(b.height, tall(300), "measured");
    assert!(list.scroll_to_row(900, RowAlign::Center));
    ui.try_prepare_frame().unwrap();
    let b = at(&ui, 900);
    assert_eq!(b.y + b.height / 2., top + 150., "its middle at the middle");
    // Nearest: already in view, nothing moves; below, it comes up to the edge.
    let before = at(&ui, 901).y;
    assert!(list.scroll_to_row(901, RowAlign::Nearest));
    ui.try_prepare_frame().unwrap();
    assert_eq!(at(&ui, 901).y, before);
    assert!(list.scroll_to_row(960, RowAlign::Nearest));
    ui.try_prepare_frame().unwrap();
    let b = at(&ui, 960);
    assert_eq!(b.y + b.height, top + 300.);
}

#[test]
fn a_fixed_list_scrolls_to_a_row_where_asked() {
    use std::cell::RefCell;
    use zgx::compose::{NodeHandle, RowAlign};
    let mut ui = Ui::new(240., 340.);
    let offset = ui.signal(0.);
    let handle: Rc<RefCell<Option<NodeHandle>>> = Rc::default();
    let slot = handle.clone();
    ui.mount(
        virtual_list(
            offset.clone(),
            20.,
            2,
            || 10_000,
            |index| index,
            |_, index, _| div().h(20.).id(format!("row-{index}")),
        )
        .size(200., 300.)
        .on_mount(move |list| *slot.borrow_mut() = Some(list)),
    );
    ui.try_prepare_frame().unwrap();
    let list = handle.borrow().clone().unwrap();
    for (align, expected) in [
        (RowAlign::Start, 5000. * 20.),
        (RowAlign::End, 5001. * 20. - 300.),
        (RowAlign::Center, 5000. * 20. + 10. - 150.),
    ] {
        assert!(list.scroll_to_row(5000, align));
        ui.try_prepare_frame().unwrap();
        assert_eq!(offset.get(), expected, "{align:?}");
    }
    // The last row, at the start, is as far as it scrolls.
    assert!(list.scroll_to_row(9999, RowAlign::Start));
    ui.try_prepare_frame().unwrap();
    assert_eq!(offset.get(), 10_000. * 20. - 300.);
}
