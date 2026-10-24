use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
};
use zgui::{
    compose::{TaskRunner, Tasks, prelude::*},
    input::InputEvent,
    reactive::Signal,
    scene::{NodeId, NodeKind},
    task::LocalExecutor,
    widgets::Ui,
};

fn content(ui: &Ui, id: NodeId) -> String {
    match ui.scene.borrow().kind(id) {
        NodeKind::Text { text, .. } => text.to_string(),
        other => panic!("expected text, got {other:?}"),
    }
}
struct DropCount(Rc<Cell<usize>>);
impl Drop for DropCount {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}
#[derive(Clone)]
struct Name(&'static str);
fn name(id: &'static str) -> View {
    component(move |cx| text(cx.service::<Name>().0).id(id))
}

#[test]
fn slots_keep_lexical_providers_and_restore_receiver_scope_for_siblings() {
    let mut ui = Ui::new(400., 200.);
    let handle = ui.mount(provide(
        Name("caller"),
        component(|cx| {
            let slot = cx.slot(|cx| text(cx.service::<Name>().0).id("slot"));
            column()
                .child(provide(
                    Name("receiver"),
                    column()
                        .child(name("before"))
                        .child(slot)
                        .child(name("after")),
                ))
                .child(name("outside"))
        }),
    ));
    for (id, expected) in [
        ("slot", "caller"),
        ("before", "receiver"),
        ("after", "receiver"),
        ("outside", "caller"),
    ] {
        assert_eq!(content(&ui, handle.find(id).unwrap()), expected);
    }
}

#[test]
fn component_styles_refine_the_actual_root_without_a_layout_wrapper() {
    let mut ui = Ui::new(400., 200.);
    let handle = ui.mount(
        row()
            .size(300., 80.)
            .child(
                component(|_| column().w(20.).child(text("child")))
                    .grow()
                    .h(60.)
                    .id("component"),
            )
            .child(column().size(100., 60.)),
    );
    ui.scene.borrow_mut().prepare_layout();
    let scene = ui.scene.borrow();
    let actual = handle.find("component").unwrap();
    assert_eq!(scene.children(handle.node())[0], actual);
    assert_eq!(scene.children(actual).len(), 1);
    assert_eq!(scene.len(), 5, "document + row + two child roots + text");
    assert_eq!(scene.style(actual).flex_grow, 1.);
    assert_eq!(scene.bounds(actual).width, 200.);
    assert_eq!(scene.bounds(actual).height, 60.);
}

#[test]
fn keyed_reorder_keeps_local_state_and_builds_only_new_keys() {
    let mut ui = Ui::new(400., 200.);
    let keys = ui.signal(vec![1, 2, 3]);
    let locals: Rc<RefCell<HashMap<i32, Signal<i32>>>> = Rc::default();
    let builds = Rc::new(Cell::new(0));
    let handle = ui.mount(keyed(
        {
            let keys = keys.clone();
            move || keys.get()
        },
        {
            let locals = locals.clone();
            let builds = builds.clone();
            move |key, cx| {
                builds.set(builds.get() + 1);
                let state = cx.state(0);
                locals.borrow_mut().insert(key, state.clone());
                text_signal(move || state.get().to_string()).id(format!("row-{key}"))
            }
        },
    ));
    let second = handle.find("row-2").unwrap();
    locals.borrow()[&2].set(42);
    keys.set(vec![3, 2, 1]);
    assert_eq!(builds.get(), 3);
    assert_eq!(handle.find("row-2"), Some(second));
    assert_eq!(content(&ui, second), "42");
    assert_eq!(
        ui.scene.borrow().children(handle.node()),
        &[
            handle.find("row-3").unwrap(),
            second,
            handle.find("row-1").unwrap()
        ]
    );
    keys.set(vec![2, 4]);
    assert_eq!(builds.get(), 4);
    assert_eq!(content(&ui, second), "42");
    assert!(handle.find("row-1").is_none());
    assert!(handle.find("row-3").is_none());
}

/// Keys read again unchanged (a list whose rows changed in place) leave
/// the children as they are; a child removed from elsewhere is built again.
#[test]
fn keyed_children_stay_when_their_keys_do() {
    let mut ui = Ui::new(400., 200.);
    let tick = ui.signal(0);
    let builds = Rc::new(Cell::new(0));
    let handle = ui.mount(keyed(
        {
            let tick = tick.clone();
            move || {
                tick.get();
                vec![1, 2, 3]
            }
        },
        {
            let builds = builds.clone();
            move |key, _| {
                builds.set(builds.get() + 1);
                text(key.to_string()).id(format!("row-{key}"))
            }
        },
    ));
    let rows = ui.scene.borrow().children(handle.node()).to_vec();
    tick.set(1);
    assert_eq!(builds.get(), 3);
    assert_eq!(ui.scene.borrow().children(handle.node()), &rows[..]);
    ui.remove(rows[1]);
    tick.set(2);
    assert_eq!(builds.get(), 4);
    let now = ui.scene.borrow().children(handle.node()).to_vec();
    assert_eq!((now.len(), now[0], now[2]), (3, rows[0], rows[2]));
    assert_ne!(now[1], rows[1]);
}

#[test]
fn conditional_replacement_disposes_resources_bindings_and_dependencies() {
    let mut ui = Ui::new(300., 200.);
    let show = ui.signal(true);
    let value = ui.signal(0);
    let reads = Rc::new(Cell::new(0));
    let drops = Rc::new(Cell::new(0));
    let clicks = Rc::new(Cell::new(0));
    let baseline = ui.runtime.effect_count();
    let handle = ui.mount(switch(
        {
            let show = show.clone();
            move || show.get()
        },
        {
            let value = value.clone();
            let reads = reads.clone();
            let drops = drops.clone();
            let clicks = clicks.clone();
            move |enabled, cx| {
                if !enabled {
                    return text("off");
                }
                cx.retain(DropCount(drops.clone()));
                let value = value.clone();
                let reads = reads.clone();
                let clicks = clicks.clone();
                button()
                    .id("action")
                    .on_click(move || clicks.set(clicks.get() + 1))
                    .child(text_signal(move || {
                        reads.set(reads.get() + 1);
                        value.get().to_string()
                    }))
            }
        },
    ));
    let action = handle.find("action").unwrap();
    ui.input
        .dispatch_to(&ui.scene, action, InputEvent::Activate);
    assert_eq!(clicks.get(), 1);
    value.set(1);
    assert_eq!(reads.get(), 2);
    show.set(false);
    assert_eq!(drops.get(), 1);
    assert!(!ui.input.has_listeners(action));
    assert!(handle.find("action").is_none());
    value.set(2);
    assert_eq!(reads.get(), 2, "removed binding must unsubscribe");
    handle.unmount();
    assert_eq!(ui.runtime.effect_count(), baseline);
    assert_eq!(ui.scene.borrow().len(), 1);
}

#[test]
fn weak_task_capabilities_do_not_keep_components_alive_or_spawn_after_unmount() {
    let mut ui = Ui::new(100., 100.);
    let executor = Rc::new(RefCell::new(LocalExecutor::new()));
    let saved: Rc<RefCell<Option<Tasks>>> = Rc::default();
    let drops = Rc::new(Cell::new(0));
    let handle = ui.mount(provide(
        TaskRunner::from_executor(executor.clone()),
        component({
            let saved = saved.clone();
            let drops = drops.clone();
            move |cx| {
                let tasks = cx.tasks();
                let captured = DropCount(drops);
                tasks.spawn(async move {
                    let _captured = captured;
                    std::future::pending::<()>().await;
                });
                *saved.borrow_mut() = Some(tasks);
                text("pending")
            }
        }),
    ));
    executor.borrow_mut().tick();
    assert!(!executor.borrow().is_empty());
    handle.unmount();
    executor.borrow_mut().tick();
    assert!(executor.borrow().is_empty());
    assert_eq!(drops.get(), 1);
    let captured = DropCount(drops.clone());
    saved.borrow().as_ref().unwrap().spawn(async move {
        let _captured = captured;
    });
    assert_eq!(drops.get(), 2);
    assert!(executor.borrow().is_empty());
    drop(ui);
    assert!(!handle.is_mounted());
}

#[test]
fn document_drop_disposes_components_even_when_handles_and_tasks_are_retained() {
    let mut ui = Ui::new(100., 100.);
    let runtime = ui.runtime.clone();
    let executor = Rc::new(RefCell::new(LocalExecutor::new()));
    let saved: Rc<RefCell<Option<Tasks>>> = Rc::default();
    let drops = Rc::new(Cell::new(0));
    let handle = ui.mount(provide(
        TaskRunner::from_executor(executor.clone()),
        component({
            let saved = saved.clone();
            let drops = drops.clone();
            move |cx| {
                cx.retain(DropCount(drops.clone()));
                let tasks = cx.tasks();
                let captured = DropCount(drops);
                tasks.spawn(async move {
                    let _captured = captured;
                    std::future::pending::<()>().await;
                });
                *saved.borrow_mut() = Some(tasks);
                text_signal(|| "alive".into())
            }
        }),
    ));
    executor.borrow_mut().tick();
    assert!(runtime.effect_count() > 0);
    drop(ui);
    assert_eq!(
        drops.get(),
        1,
        "document drop must release component resources immediately"
    );
    assert_eq!(runtime.effect_count(), 0);
    assert!(!handle.is_mounted());
    executor.borrow_mut().tick();
    assert_eq!(drops.get(), 2);
    assert!(executor.borrow().is_empty());
    drop(saved);
}

#[test]
fn reactive_properties_update_only_their_dependency_without_rebuilding_components() {
    let mut ui = Ui::new(200., 100.);
    let count = ui.signal(0);
    let title = ui.signal(String::from("stable"));
    let builds = Rc::new(Cell::new(0));
    let count_reads = Rc::new(Cell::new(0));
    let title_reads = Rc::new(Cell::new(0));
    let handle = ui.mount(component({
        let builds = builds.clone();
        let count = count.clone();
        let count_reads = count_reads.clone();
        let title_reads = title_reads.clone();
        move |_| {
            builds.set(builds.get() + 1);
            row()
                .child(
                    text_signal(move || {
                        count_reads.set(count_reads.get() + 1);
                        count.get().to_string()
                    })
                    .id("count"),
                )
                .child(
                    text_signal(move || {
                        title_reads.set(title_reads.get() + 1);
                        title.get()
                    })
                    .id("title"),
                )
        }
    }));
    let count_node = handle.find("count").unwrap();
    ui.runtime.batch(|| {
        count.set(1);
        count.set(2);
        count.set(3);
    });
    assert_eq!(builds.get(), 1);
    assert_eq!(count_reads.get(), 2);
    assert_eq!(title_reads.get(), 1);
    assert_eq!(content(&ui, count_node), "3");
    assert_eq!(handle.find("count"), Some(count_node));
    count.set(3);
    assert_eq!(count_reads.get(), 2);
    drop(handle);
    count.set(4);
    assert_eq!(
        content(&ui, count_node),
        "4",
        "Ui owns the tree when its handle is dropped"
    );
}

#[test]
fn component_root_refinement_merges_inner_and_outer_hover_properties() {
    let mut ui = Ui::new(200., 100.);
    let handle = ui.mount(
        component(|_| {
            column()
                .size(160., 80.)
                .bg(rgb(0x101010))
                .hover(|s| s.bg(rgb(0x224466)).p(6.))
                .child(text("styled").id("label"))
        })
        .hover(|s| s.text_color(rgb(0xaabbcc)).pl(11.)),
    );
    ui.input
        .dispatch_to(&ui.scene, handle.node(), InputEvent::PointerEnter);
    let scene = ui.scene.borrow();
    let NodeKind::Panel { quad, .. } = scene.kind(handle.node()) else {
        panic!("expected panel")
    };
    assert_eq!(quad.fill, rgb(0x224466));
    assert_eq!(
        scene
            .style(handle.node())
            .padding_edges
            .unwrap_or_default()
            .left,
        11.
    );
    assert_eq!(
        scene
            .style(handle.node())
            .padding_edges
            .unwrap_or_default()
            .top,
        6.
    );
    let NodeKind::Text { color, .. } = scene.kind(handle.find("label").unwrap()) else {
        panic!("expected text")
    };
    assert_eq!(*color, rgb(0xaabbcc));
    drop(scene);
    ui.input
        .dispatch_to(&ui.scene, handle.node(), InputEvent::PointerLeave);
    let scene = ui.scene.borrow();
    let NodeKind::Panel { quad, .. } = scene.kind(handle.node()) else {
        panic!("expected panel")
    };
    assert_eq!(quad.fill, rgb(0x101010));
    assert_eq!(
        scene
            .style(handle.node())
            .padding_edges
            .unwrap_or_default()
            .left,
        0.
    );
}

#[test]
fn component_root_refinement_merges_both_reactive_style_callbacks() {
    let mut ui = Ui::new(200., 100.);
    let background = ui.signal(rgb(0x112233));
    let foreground = ui.signal(rgb(0xaabbcc));
    let builds = Rc::new(Cell::new(0));
    let handle = ui.mount(
        component({
            let background = background.clone();
            let builds = builds.clone();
            move |_| {
                builds.set(builds.get() + 1);
                column()
                    .size(160., 80.)
                    .reactive_style(move || Styles::new().bg(background.get()))
                    .child(text("stable child").id("label"))
            }
        })
        .reactive_style({
            let foreground = foreground.clone();
            move || Styles::new().text_color(foreground.get())
        }),
    );
    let label = handle.find("label").unwrap();
    let assert_styles = |expected_bg, expected_fg| {
        let scene = ui.scene.borrow();
        let NodeKind::Panel { quad, .. } = scene.kind(handle.node()) else {
            panic!("expected panel")
        };
        assert_eq!(quad.fill, expected_bg);
        let NodeKind::Text { color, .. } = scene.kind(label) else {
            panic!("expected text")
        };
        assert_eq!(*color, expected_fg);
    };
    assert_styles(rgb(0x112233), rgb(0xaabbcc));
    background.set(rgb(0x334455));
    assert_styles(rgb(0x334455), rgb(0xaabbcc));
    foreground.set(rgb(0xccddee));
    assert_styles(rgb(0x334455), rgb(0xccddee));
    assert_eq!(builds.get(), 1);
    assert_eq!(handle.find("label"), Some(label));
    assert_eq!(ui.scene.borrow().len(), 3);
}

#[test]
fn component_tasks_can_spawn_component_tasks_while_executor_is_polling() {
    use zgui::compose::{TaskRunner, provide};
    let mut ui = Ui::new(200., 100.);
    let executor = Rc::new(RefCell::new(zgui::task::LocalExecutor::new()));
    let completed = Rc::new(Cell::new(false));
    let flag = completed.clone();
    let view = ui.mount(provide(
        TaskRunner::from_executor(executor.clone()),
        component(move |cx| {
            let tasks = cx.tasks();
            let nested = tasks.clone();
            tasks.spawn(async move {
                nested.spawn(async move {
                    flag.set(true);
                });
            });
            text("tasks")
        }),
    ));
    executor.borrow_mut().tick();
    assert!(!completed.get());
    executor.borrow_mut().tick();
    assert!(completed.get());
    view.unmount();
}

#[test]
fn failed_mount_cleans_partial_nodes_resources_and_preserves_existing_document() {
    struct Dropped(Rc<Cell<usize>>);
    impl Drop for Dropped {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let mut ui = Ui::new(200., 100.);
    let old = ui.mount(text("existing"));
    let before_nodes = ui.scene.borrow().len();
    let before_effects = ui.runtime.effect_count();
    let drops = Rc::new(Cell::new(0));
    let captured = drops.clone();
    let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ui.render(
            column()
                .child(component(move |cx| {
                    cx.retain(Dropped(captured));
                    button().child("partial").on_click(|| {})
                }))
                .child(component(|_| panic!("constructor failed"))),
        );
    }));
    assert!(failure.is_err());
    assert!(old.is_mounted());
    assert_eq!(drops.get(), 1);
    assert_eq!(ui.scene.borrow().len(), before_nodes);
    assert_eq!(ui.runtime.effect_count(), before_effects);
    ui.render(text("recovered"));
    assert!(!old.is_mounted());
}

#[test]
fn failed_conditional_mount_keeps_previous_branch_and_can_recover() {
    let mut ui = Ui::new(200., 100.);
    let choice = ui.signal(0);
    let read = choice.clone();
    let tree = ui.mount(zgui::compose::switch(
        move || read.get(),
        |key, _| {
            if key == 1 {
                column()
                    .child(text("partial"))
                    .child(component(|_| panic!("bad branch")))
            } else {
                text(format!("branch {key}")).id("branch")
            }
        },
    ));
    let before = tree.find("branch").unwrap();
    let nodes = ui.scene.borrow().len();
    let effects = ui.runtime.effect_count();
    let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| choice.set(1)));
    assert!(failure.is_err());
    assert_eq!(tree.find("branch"), Some(before));
    assert_eq!(ui.scene.borrow().len(), nodes);
    assert_eq!(ui.runtime.effect_count(), effects);
    choice.set(2);
    assert_ne!(tree.find("branch"), Some(before));
    tree.unmount();
    assert_eq!(ui.runtime.effect_count(), 0);
}

#[test]
fn failed_keyed_update_preserves_children_state_and_cleans_staged_mounts() {
    let mut ui = Ui::new(400., 300.);
    let keys = ui.signal(vec![1, 2]);
    let read = keys.clone();
    let fail = Rc::new(Cell::new(true));
    let should_fail = fail.clone();
    let drops = Rc::new(Cell::new(0));
    let drop_count = drops.clone();
    let states = Rc::new(RefCell::new(HashMap::new()));
    let saved_states = states.clone();
    let tree = ui.mount(keyed(
        move || read.get(),
        move |key, cx| {
            cx.retain(DropCount(drop_count.clone()));
            let state = cx.state(key * 10);
            saved_states.borrow_mut().insert(key, state.clone());
            let value = text_signal(move || state.get().to_string()).id(format!("value-{key}"));
            if key == 4 && should_fail.get() {
                column()
                    .child(value)
                    .child(component(|_| panic!("late child failure")))
            } else {
                column().id(format!("row-{key}")).child(value)
            }
        },
    ));
    let original = ui.scene.borrow().children(tree.node()).to_vec();
    let retained = tree.find("row-2").unwrap();
    states.borrow()[&2].set(222);
    let node_count = ui.scene.borrow().len();
    let effect_count = ui.runtime.effect_count();
    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| keys.set(vec![2, 3, 4])));
    assert!(failed.is_err());
    assert_eq!(ui.scene.borrow().children(tree.node()), original);
    assert_eq!(tree.find("row-2"), Some(retained));
    assert_eq!(content(&ui, tree.find("value-2").unwrap()), "222");
    assert!(tree.find("row-3").is_none());
    assert!(tree.find("value-4").is_none());
    assert_eq!(ui.scene.borrow().len(), node_count);
    assert_eq!(ui.runtime.effect_count(), effect_count);
    assert_eq!(drops.get(), 2);
    fail.set(false);
    keys.set(vec![4, 3, 2]);
    assert_eq!(tree.find("row-2"), Some(retained));
    assert_eq!(content(&ui, tree.find("value-2").unwrap()), "222");
    assert!(tree.find("row-1").is_none());
    assert_eq!(drops.get(), 3);
    let order: Vec<_> = [4, 3, 2]
        .map(|key| tree.find(&format!("row-{key}")).unwrap())
        .into();
    assert_eq!(ui.scene.borrow().children(tree.node()), order);
    tree.unmount();
    assert_eq!(drops.get(), 6);
    assert_eq!(ui.runtime.effect_count(), 0);
}
