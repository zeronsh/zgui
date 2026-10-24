//! The proposed service/provider/view/slot example, using retained Rust components.
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use zgx::{
    collections::{List, ListError},
    compose::{
        Context, Slot, TaskRunner, View, ViewHandle, button, column, component, provide, row, text,
        text_signal,
    },
    input::InputEvent,
    reactive::Signal,
    style::Styled,
    task::{LocalExecutor, yield_now},
    widgets::Ui,
};

struct Model {
    items: List<i32>,
    title: Signal<String>,
}
#[derive(Default)]
struct Audit {
    done: RefCell<Option<Signal<i32>>>,
    count_runs: Cell<usize>,
    title_runs: Cell<usize>,
}

fn count() -> View {
    component(|cx: &mut Context| {
        let model = cx.service::<Model>();
        let audit = cx.service::<Rc<Audit>>();
        text_signal(move || {
            audit.count_runs.set(audit.count_runs.get() + 1);
            model.items.len().to_string()
        })
    })
}
fn title() -> View {
    component(|cx: &mut Context| {
        let model = cx.service::<Model>();
        let audit = cx.service::<Rc<Audit>>();
        text_signal(move || {
            audit.title_runs.set(audit.title_runs.get() + 1);
            model.title.get()
        })
    })
}
fn dialog(body: Slot) -> View {
    column().child(body)
}

fn app() -> View {
    component(|cx: &mut Context| {
        let model = cx.service::<Model>();
        let done = cx.state(0);
        *cx.service::<Rc<Audit>>().done.borrow_mut() = Some(done.clone());
        let runtime = cx.runtime();
        let tasks = cx.tasks();
        let plus = button().id("plus").child(text("+")).on_click({
            let model = model.clone();
            let done = done.clone();
            let tasks = tasks.clone();
            move || {
                let model = model.clone();
                let done = done.clone();
                let runtime = runtime.clone();
                tasks.spawn(async move {
                    yield_now().await;
                    runtime.batch(|| {
                        let result = (|| {
                            model.items.append(1)?;
                            model.items.append(2)?;
                            model.items.append(3)?;
                            Ok::<_, ListError>(())
                        })();
                        done.set(match result {
                            Ok(()) => 0,
                            Err(ListError::Capacity { .. }) => -1,
                            Err(ListError::Exhausted) => -2,
                            Err(error) => panic!("unexpected append error: {error}"),
                        });
                    });
                });
            }
        });
        let equals = button().id("equals").child(text("=")).on_click({
            let model = model.clone();
            let done = done.clone();
            move || {
                let model = model.clone();
                let done = done.clone();
                tasks.spawn(async move {
                    yield_now().await;
                    let result = (|| {
                        let item = model.items.at(0)?;
                        item.write(99)?;
                        item.write(99)?;
                        Ok::<_, ListError>(())
                    })();
                    done.set(match result {
                        Ok(()) => 0,
                        Err(ListError::Bounds { .. }) => -1,
                        Err(ListError::RowRemoved) => -2,
                        Err(error) => panic!("unexpected row error: {error}"),
                    });
                });
            }
        });
        let done_button = button()
            .id("done")
            .child(text_signal({
                let done = done.clone();
                move || done.get().to_string()
            }))
            .on_click(move || {
                done.set(999);
            });
        let body = cx.slot(|cx| {
            let model = cx.service::<Model>();
            text_signal(move || model.items.len().to_string())
        });
        column()
            .gap(8.0)
            .p(12.0)
            .child(row().gap(8.0).child(plus).child(equals).child(done_button))
            .child(
                row()
                    .gap(16.0)
                    .child(count())
                    .child(title())
                    .child(dialog(body)),
            )
    })
}

// A headless host for the example and its behavioral checks. Native windows use
// WindowContext::render with the same component tree and provide a task runner.
struct Demo {
    ui: Ui,
    tree: ViewHandle,
    model: Model,
    audit: Rc<Audit>,
    executor: Rc<RefCell<LocalExecutor>>,
}
impl Demo {
    fn new() -> Self {
        let mut ui = Ui::new(600.0, 200.0);
        let model = Model {
            items: List::new(&ui.runtime, 4),
            title: ui.runtime.signal("stable".to_owned()),
        };
        let audit = Rc::new(Audit::default());
        let executor = Rc::new(RefCell::new(LocalExecutor::new()));
        let tree = ui.mount(provide(
            TaskRunner::from_executor(executor.clone()),
            provide(
                audit.clone(),
                provide(
                    Model {
                        items: model.items.clone(),
                        title: model.title.clone(),
                    },
                    app(),
                ),
            ),
        ));
        Self {
            ui,
            tree,
            model,
            audit,
            executor,
        }
    }
    fn click(&self, name: &str) {
        self.ui.input.dispatch_to(
            &self.ui.scene,
            self.tree.find(name).unwrap(),
            InputEvent::Activate,
        );
    }
    fn drain(&self) {
        while self.executor.borrow().has_ready() {
            self.executor.borrow_mut().tick();
        }
    }
    fn done(&self) -> i32 {
        self.audit.done.borrow().as_ref().unwrap().get()
    }
}
fn main() {
    let demo = Demo::new();
    demo.ui.scene.borrow_mut().flush();
    demo.click("plus");
    demo.drain();
    let append_frame = demo.ui.scene.borrow_mut().flush();
    demo.click("equals");
    demo.drain();
    let row_frame = demo.ui.scene.borrow_mut().flush();
    demo.click("done");
    println!(
        "count={}, title={}, done={}, count view runs={}, title view runs={}",
        demo.model.items.len(),
        demo.model.title.get(),
        demo.done(),
        demo.audit.count_runs.get(),
        demo.audit.title_runs.get()
    );
    println!(
        "append damage regions={}, row edit scene idle={}",
        append_frame.damage.len(),
        row_frame.is_idle()
    );
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn components_preserve_sample_updates_and_partial_append_errors() {
        let demo = Demo::new();
        demo.ui.scene.borrow_mut().flush();
        demo.click("plus");
        demo.executor.borrow_mut().tick();
        assert_eq!(demo.model.items.len(), 0);
        demo.executor.borrow_mut().tick();
        assert_eq!(demo.model.items.len(), 3);
        assert_eq!(demo.audit.count_runs.get(), 2);
        assert_eq!(demo.audit.title_runs.get(), 1);
        demo.ui.scene.borrow_mut().flush();
        let item = demo.model.items.at(0).unwrap();
        let writes = Rc::new(Cell::new(0));
        let _watch = demo.ui.runtime.effect({
            let item = item.clone();
            let writes = writes.clone();
            move || {
                item.read().unwrap();
                writes.set(writes.get() + 1);
            }
        });
        demo.click("equals");
        demo.drain();
        assert_eq!(item.read(), Ok(99));
        assert_eq!(writes.get(), 2);
        assert!(demo.ui.scene.borrow_mut().flush().is_idle());
        assert_eq!(demo.audit.count_runs.get(), 2);
        assert_eq!(demo.audit.title_runs.get(), 1);
        demo.click("done");
        assert_eq!(demo.done(), 999);
        demo.click("plus");
        demo.drain();
        assert_eq!(demo.done(), -1);
        assert_eq!(demo.model.items.len(), 4);
    }
    #[test]
    fn empty_row_access_maps_to_bounds_error() {
        let demo = Demo::new();
        demo.click("equals");
        demo.drain();
        assert_eq!(demo.done(), -1);
    }
    #[test]
    fn unmount_cancels_yielding_click_handler() {
        let demo = Demo::new();
        demo.click("plus");
        demo.executor.borrow_mut().tick();
        demo.tree.unmount();
        demo.drain();
        assert_eq!(demo.model.items.len(), 0);
        assert_eq!(demo.ui.runtime.effect_count(), 0);
    }
}
