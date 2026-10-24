//! `ui.every(ms, fn)` and `ui.after(ms, fn)`: timers on zgx's own clock
//! that wake the app only when they fire (no frames between), owned like
//! `ui.effect`: by the component or builder that made them, else the script,
//! so a reload or an unmount stops them.
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
    time::Duration,
};

type Pending = Pin<Box<dyn Future<Output = ()>>>;

/// Where timers run: the host view's tasks, once it mounts (a script makes
/// its timers while it loads, before that; they wait here).
#[derive(Default)]
pub(crate) struct Timers {
    tasks: RefCell<Option<zgx::compose::Tasks>>,
    pending: RefCell<Vec<Pending>>,
}
impl Timers {
    pub fn set_tasks(&self, tasks: zgx::compose::Tasks) {
        for task in self.pending.take() {
            tasks.spawn(task);
        }
        *self.tasks.borrow_mut() = Some(tasks);
    }
    pub(crate) fn spawn(&self, task: impl Future<Output = ()> + 'static) {
        match &*self.tasks.borrow() {
            Some(tasks) => tasks.spawn(task),
            None => self.pending.borrow_mut().push(Box::pin(task)),
        }
    }
    /// Call `fire` after `period`, then every `period` while `repeat`, until
    /// the returned guard drops or `fire` says stop (returns false).
    pub fn start(
        &self,
        period: Duration,
        repeat: bool,
        mut fire: impl FnMut() -> bool + 'static,
    ) -> Stop {
        let alive = Rc::new(Cell::new(true));
        let running = alive.clone();
        self.spawn(async move {
            loop {
                zgx::timer::sleep(period).await;
                if !running.get() || !fire() || !repeat {
                    return;
                }
            }
        });
        Stop(alive)
    }
}
/// Stops its timer when dropped (or `stop`ped).
pub(crate) struct Stop(Rc<Cell<bool>>);
impl Stop {
    pub fn handle(&self) -> Rc<Cell<bool>> {
        self.0.clone()
    }
}
impl Drop for Stop {
    fn drop(&mut self) {
        self.0.set(false);
    }
}
