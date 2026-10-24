use std::{cell::Cell, rc::Rc};
use zgui::{compose::prelude::*, scene::NodeKind, widgets::Ui};

struct DropCount(Rc<Cell<usize>>);
impl Drop for DropCount {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}
#[test]
fn virtual_rows_are_bounded_retain_keys_and_dispose_offscreen_state() {
    let mut ui = Ui::new(400., 300.);
    let offset = ui.signal(0.);
    let builds = Rc::new(Cell::new(0));
    let drops = Rc::new(Cell::new(0));
    let key_calls = Rc::new(Cell::new(0));
    let builds2 = builds.clone();
    let drops2 = drops.clone();
    let calls = key_calls.clone();
    let handle = ui.mount(provide(
        String::from("row"),
        virtual_list(
            offset.clone(),
            20.,
            1,
            || 100_000,
            move |i| {
                calls.set(calls.get() + 1);
                i
            },
            move |index, key, cx| {
                builds2.set(builds2.get() + 1);
                cx.retain(DropCount(drops2.clone()));
                let prefix = cx.service::<String>();
                text_signal(move || format!("{prefix} {}", index.get())).id(format!("row-{key}"))
            },
        )
        .w(200.)
        .h(100.),
    ));
    ui.prepare_frame();
    assert_eq!(builds.get(), 6);
    let retained = handle.find("row-1").unwrap();
    offset.set(20.);
    ui.prepare_frame();
    assert_eq!(handle.find("row-1"), Some(retained));
    assert_eq!(builds.get(), 7);
    offset.set(20_000.);
    ui.prepare_frame();
    assert!(handle.find("row-1").is_none());
    assert_eq!(builds.get() - drops.get(), 7);
    assert!(
        key_calls.get() < 50,
        "key work must scale with viewport, not count"
    );
    handle.unmount();
    assert_eq!(builds.get(), drops.get());
    let before = builds.get();
    offset.set(0.);
    assert_eq!(builds.get(), before);
}
#[test]
fn reorder_updates_index_without_rebuilding_key_and_resize_expands_range() {
    let mut ui = Ui::new(400., 300.);
    let offset = ui.signal(0.);
    let order = ui.signal(vec![10, 11, 12, 13, 14]);
    let count = order.clone();
    let keys = order.clone();
    let height = ui.signal(40.);
    let style_height = height.clone();
    let handle = ui.mount(
        virtual_list(
            offset.clone(),
            20.,
            0,
            move || count.with(Vec::len),
            move |i| keys.with(|v| v[i]),
            move |index, key, _| text_signal(move || index.get().to_string()).id(format!("{key}")),
        )
        .w(200.)
        .reactive_style(move || Styles::new().h(style_height.get())),
    );
    ui.prepare_frame();
    let original = handle.find("10").unwrap();
    order.set(vec![11, 10, 12, 13, 14]);
    assert_eq!(handle.find("10"), Some(original));
    match ui.scene.borrow().kind(original) {
        NodeKind::Text { text, .. } => assert_eq!(text.as_ref(), "1"),
        _ => panic!(),
    }
    height.set(80.);
    ui.prepare_frame();
    assert!(handle.find("13").is_some());
    assert!(handle.find("14").is_none());
    offset.set(1000.);
    ui.prepare_frame();
    assert_eq!(offset.get(), 20.);
    order.set(vec![]);
    ui.prepare_frame();
    assert_eq!(offset.get(), 0.);
    assert!(handle.find("10").is_none());
}
#[test]
fn wheel_scrolls_and_failed_constructor_preserves_existing_rows() {
    let mut ui = Ui::new(200., 100.);
    let offset = ui.signal(0.);
    let fail = Rc::new(Cell::new(true));
    let fail_build = fail.clone();
    let handle = ui.mount(
        virtual_list(
            offset.clone(),
            20.,
            0,
            || 100,
            |i| i,
            move |_, key, _| {
                assert!(!(key == 3 && fail_build.get()), "row construction failed");
                text(format!("{key}")).id(format!("{key}"))
            },
        )
        .w(200.)
        .h(40.),
    );
    ui.prepare_frame();
    let first = handle.find("0").unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| offset.set(40.)));
    assert!(result.is_err());
    assert_eq!(handle.find("0"), Some(first));
    assert!(
        handle.find("2").is_none(),
        "partially constructed row is cleaned up"
    );
    fail.set(false);
    offset.set(0.);
    ui.dispatch(zgui::input::InputEvent::Scroll {
        x: 10.,
        y: 10.,
        delta_x: 0.,
        delta_y: 20.,
    });
    assert_eq!(offset.get(), 20.);
    assert!(handle.find("2").is_some());
    offset.set(f32::NAN);
    assert_eq!(offset.get(), 0.);
}
#[test]
fn scrolling_with_same_visible_rows_changes_only_composition() {
    let mut ui = Ui::new(200., 100.);
    let offset = ui.signal(1.);
    ui.mount(
        virtual_list(
            offset.clone(),
            20.,
            1,
            || 100,
            |i| i,
            |_, key, _| text(key.to_string()),
        )
        .w(200.)
        .h(40.),
    );
    ui.prepare_frame();
    ui.scene.borrow_mut().flush();
    offset.set(2.);
    ui.prepare_frame();
    let report = ui.scene.borrow_mut().flush();
    assert_eq!(report.layout_nodes, 0);
    assert!(report.composite_nodes > 0);
}

#[test]
fn virtual_list_initial_disabled_semantics_and_wheel_agree() {
    use zgui::{input::InputEvent, semantics::Role};
    let mut ui = Ui::new(400., 300.);
    let offset = ui.signal(0.);
    let mounted = ui.mount(
        virtual_list(
            offset.clone(),
            20.,
            1,
            || 100,
            |i| i,
            |_, i, _| text(i.to_string()),
        )
        .disabled(true),
    );
    ui.prepare_frame();
    let root = mounted.node();
    let semantics = ui.semantics.borrow().get(root).unwrap().clone();
    assert_eq!(semantics.role, Role::ScrollView);
    assert!(semantics.disabled);
    ui.input.dispatch_to(
        &ui.scene,
        root,
        InputEvent::Scroll {
            x: 10.,
            y: 10.,
            delta_x: 0.,
            delta_y: 50.,
        },
    );
    assert_eq!(offset.get(), 0.);
    offset.set(50.);
    ui.prepare_frame();
    assert_eq!(offset.get(), 50.);
    assert!(ui.semantics.borrow().get(root).unwrap().disabled);
}

#[test]
fn callback_scroll_requests_survive_stale_range_writeback() {
    for from_key in [true, false] {
        let mut ui = Ui::new(400., 300.);
        let offset = ui.signal(0.);
        let once = Rc::new(Cell::new(false));
        let key_once = once.clone();
        let key_offset = offset.clone();
        let build_offset = offset.clone();
        let handle = ui.mount(
            virtual_list(
                offset.clone(),
                20.,
                0,
                || 100,
                move |index| {
                    if from_key && !key_once.replace(true) {
                        key_offset.set(200.);
                    }
                    index
                },
                move |_, index, _| {
                    if !from_key && !once.replace(true) {
                        build_offset.set(200.);
                    }
                    text(format!("Row {index}")).id(format!("callback-row-{index}"))
                },
            )
            .w(200.)
            .h(60.)
            .keyboard_navigation(true),
        );
        ui.prepare_frame();
        assert_eq!(offset.get(), 200., "from_key={from_key}");
        assert!(handle.find("callback-row-10").is_some());
        assert!(handle.find("callback-row-0").is_none());
    }
}

#[test]
fn a_virtual_list_sizes_as_a_scroll_view_does() {
    // Unsized, 320 by 240; stretched by its column, as wide as it; its own
    // size holds.
    let mut ui = Ui::new(600., 600.);
    let offsets: Vec<_> = (0..3).map(|_| ui.signal(0_f32)).collect();
    let list = |at: usize, id: &str| {
        virtual_list(
            offsets[at].clone(),
            20.,
            1,
            || 50,
            |i| i,
            |_, i, _| text(format!("{i}")),
        )
        .id(id.to_owned())
    };
    let view = ui.mount(
        column()
            .w(500.)
            .child(row().child(list(0, "alone")))
            .child(list(1, "stretched"))
            .child(list(2, "sized").w(200.).h(100.)),
    );
    ui.prepare_frame();
    let size = |id: &str| {
        let b = ui.scene.borrow().bounds(view.find(id).unwrap());
        (b.width, b.height)
    };
    assert_eq!(size("alone"), (320., 240.));
    assert_eq!(size("stretched"), (500., 240.));
    assert_eq!(size("sized"), (200., 100.));
}
