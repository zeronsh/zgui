//! Cancellation ownership for native UI tasks, including tasks not yet polled.
use std::{
    cell::RefCell,
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};
type FutureTask = Pin<Box<dyn Future<Output = ()>>>;
struct State {
    future: Option<FutureTask>,
    finished: bool,
    waker: Option<Waker>,
}
fn cancel(state: &Rc<RefCell<State>>) {
    let (future, waker) = {
        let mut state = state.borrow_mut();
        if state.finished {
            return;
        }
        state.finished = true;
        (state.future.take(), state.waker.take())
    };
    // Destructors may cancel other resources, so run them outside the state borrow.
    drop(future);
    if let Some(waker) = waker {
        waker.wake();
    }
}
/// Dropping this guard cancels its task and releases the future's captures.
/// Retain it in a `ViewScope` to bind a native task to a mounted subtree.
#[must_use = "The task is canceled when this guard is dropped"]
pub struct ScopedTask {
    state: Rc<RefCell<State>>,
}
impl ScopedTask {
    /// Cancel the task now: it won't be polled again and its future is dropped.
    pub fn cancel(&self) {
        cancel(&self.state);
    }
    /// Whether the task has finished or been cancelled.
    pub fn is_finished(&self) -> bool {
        self.state.borrow().finished
    }
}
impl Drop for ScopedTask {
    fn drop(&mut self) {
        self.cancel();
    }
}
pub(crate) struct Cancellable {
    state: Rc<RefCell<State>>,
}
pub(crate) fn cancellable(future: impl Future<Output = ()> + 'static) -> (ScopedTask, Cancellable) {
    let state = Rc::new(RefCell::new(State {
        future: Some(Box::pin(future)),
        finished: false,
        waker: None,
    }));
    (
        ScopedTask {
            state: state.clone(),
        },
        Cancellable { state },
    )
}
impl Future for Cancellable {
    type Output = ();
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let future = {
            let mut state = self.state.borrow_mut();
            if state.finished {
                return Poll::Ready(());
            }
            state.waker = Some(cx.waker().clone());
            state.future.take()
        };
        let Some(mut future) = future else {
            return Poll::Ready(());
        };
        let result = future.as_mut().poll(cx);
        let mut state = self.state.borrow_mut();
        if result.is_ready() {
            state.finished = true;
            state.waker = None;
            Poll::Ready(())
        } else if state.finished {
            state.waker = None;
            Poll::Ready(())
        } else {
            state.future = Some(future);
            Poll::Pending
        }
    }
}
impl Drop for Cancellable {
    fn drop(&mut self) {
        cancel(&self.state);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use zgx::{
        reactive::Runtime,
        scene::{Layout, Scene, Style},
        task::LocalExecutor,
        view::ViewScope,
    };
    struct OnDrop(Rc<Cell<usize>>);
    impl Drop for OnDrop {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    #[test]
    fn completed_future_destructor_can_reenter_its_cancellation_state() {
        struct Reentrant {
            slot: Rc<RefCell<Option<ScopedTask>>>,
        }
        impl Future for Reentrant {
            type Output = ();
            fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<()> {
                Poll::Ready(())
            }
        }
        impl Drop for Reentrant {
            fn drop(&mut self) {
                self.slot.borrow().as_ref().unwrap().cancel();
            }
        }
        let slot = Rc::new(RefCell::new(None));
        let (guard, task) = cancellable(Reentrant { slot: slot.clone() });
        *slot.borrow_mut() = Some(guard);
        let mut executor = LocalExecutor::new();
        executor.spawn(task);
        executor.tick();
        assert!(slot.borrow().as_ref().unwrap().is_finished());
    }
    #[test]
    fn cancel_before_executor_poll_releases_captures() {
        let dropped = Rc::new(Cell::new(0));
        let capture = OnDrop(dropped.clone());
        let (guard, task) = cancellable(async move {
            let _capture = capture;
            std::future::pending::<()>().await;
        });
        drop(guard);
        assert_eq!(dropped.get(), 1);
        let mut executor = LocalExecutor::new();
        executor.spawn(task);
        assert_eq!(executor.tick(), 1);
        assert!(!executor.has_ready());
    }
    #[test]
    fn view_disposal_wakes_sleeping_task_and_drops_capture_immediately() {
        let runtime = Runtime::new();
        let scene = Rc::new(RefCell::new(Scene::new(100., 100.)));
        let root = scene.borrow().root();
        let mut view = ViewScope::mount(
            &runtime,
            scene.clone(),
            root,
            Layout::Column,
            Style::default(),
        );
        let dropped = Rc::new(Cell::new(0));
        let capture = OnDrop(dropped.clone());
        let (guard, task) = cancellable(async move {
            let _capture = capture;
            std::future::pending::<()>().await;
        });
        view.retain(guard);
        let mut executor = LocalExecutor::new();
        executor.spawn(task);
        executor.tick();
        assert!(!executor.has_ready());
        drop(view);
        assert_eq!(dropped.get(), 1);
        assert!(executor.has_ready());
        executor.tick();
        assert!(!executor.has_ready());
        assert_eq!(scene.borrow().len(), 1);
    }
    #[test]
    fn window_executor_drop_cancels_even_when_guard_survives() {
        let dropped = Rc::new(Cell::new(0));
        let capture = OnDrop(dropped.clone());
        let (guard, task) = cancellable(async move {
            let _capture = capture;
            std::future::pending::<()>().await;
        });
        let mut executor = LocalExecutor::new();
        executor.spawn(task);
        executor.tick();
        drop(executor);
        assert!(guard.is_finished());
        assert_eq!(dropped.get(), 1);
    }
    #[test]
    fn self_cancellation_during_poll_is_safe() {
        let slot: Rc<RefCell<Option<ScopedTask>>> = Rc::new(RefCell::new(None));
        let own = slot.clone();
        let (guard, task) = cancellable(async move {
            own.borrow_mut().take();
            std::future::pending::<()>().await;
        });
        *slot.borrow_mut() = Some(guard);
        let mut executor = LocalExecutor::new();
        executor.spawn(task);
        executor.tick();
        executor.tick();
        assert!(slot.borrow().is_none());
        assert!(!executor.has_ready());
    }
}
