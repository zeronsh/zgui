use std::{cell::Cell, rc::Rc};
use zgx::{
    compose::{ViewHandle, prelude::*},
    widgets::Ui,
};

struct Lease(Rc<Cell<usize>>);
impl Lease {
    fn new(live: Rc<Cell<usize>>) -> Self {
        live.set(live.get() + 1);
        Self(live)
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        self.0.set(self.0.get() - 1);
    }
}
struct Service(Rc<()>);

#[test]
fn retained_handles_do_not_keep_keyed_rows_slots_services_or_effects_alive() {
    let mut ui = Ui::new(400., 300.);
    let baseline = ui.runtime.effect_count();
    let live = Rc::new(Cell::new(0));
    let mut old_handles = Vec::new();
    for _ in 0..20 {
        let token = Rc::new(());
        let weak = Rc::downgrade(&token);
        let keys = ui.signal(vec![1, 2, 3]);
        let read_keys = keys.clone();
        let tick = ui.signal(0);
        let read_tick = tick.clone();
        let live_rows = live.clone();
        let mounted = ui.mount(provide(
            Service(token),
            component(move |cx| {
                let body = cx.slot(move |cx| {
                    let service = cx.service::<Service>();
                    text_signal(move || {
                        format!("{}:{}", Rc::strong_count(&service.0), read_tick.get())
                    })
                    .id("slot")
                });
                column().child(body).child(keyed(
                    move || read_keys.get(),
                    move |key, cx| {
                        cx.retain(Lease::new(live_rows.clone()));
                        text(format!("row {key}")).id(format!("row-{key}"))
                    },
                ))
            }),
        ));
        ui.prepare_frame();
        let retained = mounted.find("row-2");
        assert_eq!(live.get(), 3);
        keys.set(vec![3, 2, 1]);
        ui.prepare_frame();
        assert_eq!(mounted.find("row-2"), retained);
        assert_eq!(live.get(), 3);
        keys.set(vec![2, 4]);
        ui.prepare_frame();
        assert_eq!(live.get(), 2);
        assert_eq!(mounted.find("row-2"), retained);
        mounted.unmount();
        ui.prepare_frame();
        assert_eq!(live.get(), 0);
        assert!(
            weak.upgrade().is_none(),
            "slot/provider ownership must end with subtree"
        );
        assert_eq!(ui.runtime.effect_count(), baseline);
        keys.set(vec![5]);
        tick.set(1);
        assert!(!mounted.is_mounted());
        assert!(mounted.find("slot").is_none());
        old_handles.push(mounted);
    }
    assert_eq!(ui.scene.borrow().children(ui.root()).len(), 0);
    assert_eq!(live.get(), 0);
    for handle in old_handles {
        handle.unmount();
    }
}

struct RemoveOnDrop(ViewHandle, Rc<Cell<usize>>);
impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        self.1.set(self.1.get() + 1);
        self.0.unmount();
    }
}

#[test]
fn component_resource_destructor_can_remove_sibling_with_retained_handles() {
    let mut ui = Ui::new(400., 300.);
    let baseline = ui.runtime.effect_count();
    let live = Rc::new(Cell::new(0));
    let child_live = live.clone();
    let sibling = ui.mount(component(move |cx| {
        cx.retain(Lease::new(child_live));
        text("Sibling").id("sibling")
    }));
    let calls = Rc::new(Cell::new(0));
    let count = calls.clone();
    let remove = sibling.clone();
    let owner = ui.mount(component(move |cx| {
        cx.retain(RemoveOnDrop(remove, count));
        text("Owner")
    }));
    owner.unmount();
    ui.prepare_frame();
    assert_eq!(calls.get(), 1);
    assert_eq!(live.get(), 0);
    assert!(!owner.is_mounted());
    assert!(!sibling.is_mounted());
    assert!(sibling.find("sibling").is_none());
    assert_eq!(ui.runtime.effect_count(), baseline);
    owner.unmount();
    sibling.unmount();
    assert_eq!(calls.get(), 1);
}

#[test]
fn completed_task_guard_destructor_can_spawn_through_retained_tasks_capability() {
    use std::cell::RefCell;
    use zgx::compose::{TaskRunner, TaskToken, Tasks};
    struct Reenter {
        tasks: Rc<RefCell<Option<Tasks>>>,
        fired: Rc<Cell<bool>>,
    }
    impl Drop for Reenter {
        fn drop(&mut self) {
            if !self.fired.replace(true) {
                self.tasks.borrow().as_ref().unwrap().spawn(async {});
            }
        }
    }
    let tasks = Rc::new(RefCell::new(None::<Tasks>));
    let output = tasks.clone();
    let runner_tasks = tasks.clone();
    let fired = Rc::new(Cell::new(false));
    let observed = fired.clone();
    let runner = TaskRunner::new(move |_future| {
        TaskToken::new(
            Reenter {
                tasks: runner_tasks.clone(),
                fired: observed.clone(),
            },
            |_| true,
        )
    });
    let mut ui = Ui::new(400., 300.);
    let mounted = ui.mount(provide(
        runner,
        component(move |cx| {
            *output.borrow_mut() = Some(cx.tasks());
            text("Tasks")
        }),
    ));
    let capability = tasks.borrow().as_ref().unwrap().clone();
    capability.spawn(async {});
    capability.spawn(async {});
    assert!(fired.get());
    mounted.unmount();
    tasks.borrow_mut().take();
    capability.spawn(async {});
}

#[test]
fn task_completion_check_can_spawn_and_new_guard_stays_owned_until_unmount() {
    use std::cell::RefCell;
    use zgx::compose::{TaskRunner, TaskToken, Tasks};
    let mut ui = Ui::new(400., 300.);
    let tasks = Rc::new(RefCell::new(None::<Tasks>));
    let output = tasks.clone();
    let runner_tasks = tasks.clone();
    let checked = Rc::new(Cell::new(false));
    let live = Rc::new(Cell::new(0));
    let live_guards = live.clone();
    let observed = checked.clone();
    let runner = TaskRunner::new(move |_future| {
        let tasks = runner_tasks.clone();
        let checked = observed.clone();
        TaskToken::new(Lease::new(live_guards.clone()), move |_| {
            if !checked.replace(true) {
                tasks.borrow().as_ref().unwrap().spawn(async {});
            }
            false
        })
    });
    let mounted = ui.mount(provide(
        runner,
        component(move |cx| {
            *output.borrow_mut() = Some(cx.tasks());
            text("Tasks")
        }),
    ));
    let capability = tasks.borrow().as_ref().unwrap().clone();
    capability.spawn(async {});
    assert_eq!(live.get(), 1);
    capability.spawn(async {});
    assert!(checked.get());
    assert_eq!(
        live.get(),
        3,
        "original, reentrant and outer tasks remain owned"
    );
    mounted.unmount();
    assert_eq!(live.get(), 0, "unmount releases every live guard");
    tasks.borrow_mut().take();
    capability.spawn(async {});
    assert_eq!(
        live.get(),
        0,
        "retained capability cannot recreate a dead owner"
    );
}

#[test]
fn spawning_many_tasks_checks_each_a_bounded_number_of_times() {
    use std::cell::RefCell;
    use zgx::compose::{TaskRunner, TaskToken, Tasks};
    // A burst of spawns while none finish (a script calling the host in a
    // loop): pruning finished tasks must not rescan every task per spawn.
    let mut ui = Ui::new(400., 300.);
    let tasks = Rc::new(RefCell::new(None::<Tasks>));
    let output = tasks.clone();
    let checks = Rc::new(Cell::new(0usize));
    let counted = checks.clone();
    let runner = TaskRunner::new(move |_future| {
        let counted = counted.clone();
        TaskToken::new((), move |_| {
            counted.set(counted.get() + 1);
            false
        })
    });
    let _mounted = ui.mount(provide(
        runner,
        component(move |cx| {
            *output.borrow_mut() = Some(cx.tasks());
            text("Tasks")
        }),
    ));
    let capability = tasks.borrow().as_ref().unwrap().clone();
    const N: usize = 5_000;
    for _ in 0..N {
        capability.spawn(async {});
    }
    assert!(
        checks.get() <= 3 * N,
        "{} finished-checks for {N} spawns",
        checks.get()
    );
}

#[test]
fn a_large_removal_stops_its_effects_at_once_and_frees_them_over_frames() {
    // Removing a thousand rows: their effects stop at once (a write they
    // read runs none of them), and what they hold is freed over the frames
    // that follow, within a bounded number, while new rows mount meanwhile.
    let mut ui = Ui::new(800., 600.);
    let tick = ui.signal(0_u32);
    let runs = Rc::new(Cell::new(0usize));
    let live = Rc::new(Cell::new(0usize));
    let rows = |n: usize| {
        let (tick, runs, live) = (tick.clone(), runs.clone(), live.clone());
        column().children((0..n).map(move |_| {
            let (tick, runs) = (tick.clone(), runs.clone());
            let lease = Lease::new(live.clone());
            text_signal(move || {
                let _held = &lease;
                runs.set(runs.get() + 1);
                tick.get().to_string()
            })
        }))
    };
    for round in 0..3 {
        let mounted = ui.mount(rows(1000));
        ui.prepare_frame();
        assert!(live.get() >= 1000, "round {round}: mounted");
        mounted.unmount();
        assert!(ui.reclaiming(), "round {round}: freed later, not now");
        let ran = runs.get();
        tick.update(|t| *t += 1);
        ui.prepare_frame();
        assert_eq!(runs.get(), ran, "round {round}: no removed row ran again");
    }
    let mut frames = 0;
    while ui.reclaiming() {
        ui.prepare_frame();
        frames += 1;
        assert!(frames < 200, "freed within a bounded number of frames");
    }
    assert_eq!(live.get(), 0, "everything the rows held is freed");
}

/// Writes its signal when dropped: a listener's or a component's resource
/// whose destructor changes state.
struct WriteOnDrop(zgx::reactive::Signal<u32>);
impl Drop for WriteOnDrop {
    fn drop(&mut self) {
        self.0.set(1);
    }
}

/// Removes `children` children under a parent whose listener's destructor
/// writes a signal they read: none of them runs again, removed.
fn removed_descendants_stop_before_listener_destructors(children: usize) {
    use zgx::scene::{Layout, Style};
    let mut ui = Ui::new(400., 300.);
    let parent = ui.container(ui.root(), Layout::Column, Style::default());
    let tick = ui.signal(0_u32);
    let removed_runs = Rc::new(Cell::new(0));
    for _ in 0..children {
        let node = ui.container(parent, Layout::Column, Style::default());
        let (signal, scene, count) = (tick.clone(), ui.scene.clone(), removed_runs.clone());
        ui.bind(node, move || {
            signal.get();
            if !scene.borrow().contains(node) {
                count.set(count.get() + 1);
            }
        });
    }
    let guard = WriteOnDrop(tick.clone());
    ui.on_event(parent, false, move |_| {
        let _ = &guard;
    });
    ui.remove(parent);
    while ui.reclaiming() {
        ui.reclaim(std::time::Duration::ZERO);
    }
    assert_eq!(tick.get(), 1);
    assert_eq!(
        removed_runs.get(),
        0,
        "removed descendants must stop before listener destructors write state"
    );
    assert_eq!(ui.runtime.effect_count(), 0);
}

#[test]
fn a_small_removal_stops_descendants_before_listener_destructors() {
    removed_descendants_stop_before_listener_destructors(1);
}

#[test]
fn a_large_removal_stops_descendants_before_listener_destructors() {
    removed_descendants_stop_before_listener_destructors(300);
}

#[test]
fn removing_a_focused_large_tree_lets_a_listener_destructor_remove_a_node() {
    use zgx::scene::{Layout, Style};
    struct RemoveOnDrop(zgx::widgets::WeakUi, zgx::scene::NodeId);
    impl Drop for RemoveOnDrop {
        fn drop(&mut self) {
            if let Some(mut ui) = self.0.upgrade() {
                ui.remove(self.1);
            }
        }
    }
    let mut ui = Ui::new(400., 300.);
    let sibling = ui.container(ui.root(), Layout::Column, Style::default());
    let parent = ui.container(ui.root(), Layout::Column, Style::default());
    for _ in 0..300 {
        ui.container(parent, Layout::Column, Style::default());
    }
    let guard = RemoveOnDrop(ui.downgrade(), sibling);
    ui.on_event(parent, true, move |_| {
        let _ = &guard;
    });
    assert!(ui.input.focus(&ui.scene, Some(parent)));
    ui.remove(parent);
    assert!(!ui.scene.borrow().contains(sibling));
}

#[test]
fn component_cleanup_stops_descendant_effects_before_its_resources_drop() {
    let mut ui = Ui::new(400., 300.);
    let tick = ui.signal(0_u32);
    let cleanup_tick = tick.clone();
    let view = ui.mount(component(move |cx| {
        cx.retain(WriteOnDrop(cleanup_tick));
        column().child(div().id("child"))
    }));
    let child = view.find("child").unwrap();
    let after_removal = Rc::new(Cell::new(0));
    let (scene, count, signal) = (ui.scene.clone(), after_removal.clone(), tick.clone());
    ui.bind(child, move || {
        signal.get();
        if !scene.borrow().contains(child) {
            count.set(count.get() + 1);
        }
    });
    view.unmount();
    assert_eq!(tick.get(), 1);
    assert_eq!(after_removal.get(), 0);
}
