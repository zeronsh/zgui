# Tasks owned by mounted views

`WindowContext.tasks.spawn` runs until completion or its window closes. Use `spawn_scoped` when a task belongs to a view that may disappear first. Keep its returned `ScopedTask` guard in the view's resource list:

```rust
use zgx::{scene::{Layout, Style}, view::ViewScope};
use zgx_desktop::WindowContext;

fn mount_status(cx: &mut WindowContext) -> ViewScope {
    let mut scope = ViewScope::mount(
        &cx.ui.runtime, cx.ui.scene.clone(), cx.ui.root(),
        Layout::Column, Style::default(),
    );
    let status = cx.ui.signal(String::from("Loading"));
    scope.text(Style::default(), cx.ui.theme.text, 16.0, {
        let status = status.clone();
        move || status.get()
    });
    scope.retain(cx.tasks.spawn_scoped(async move {
        zgx::timer::sleep(std::time::Duration::from_secs(1)).await;
        status.set(String::from("Ready"));
    }));
    scope
}
```

The caller retains the returned scope while it is mounted. Dropping the scope cancels the task before removing its nodes. Cancellation releases the future's captures immediately, including when the task has not reached the executor yet, and wakes a sleeping executor entry for cleanup. A task currently inside its synchronous poll finishes that poll before its future is dropped. Closing the window also cancels the task, even if another owner retains the guard.

`ScopedTask::cancel` explicitly cancels; `is_finished` reports completion or cancellation. Dropping an unretained guard cancels immediately. These futures run on the UI thread: use the background executor for blocking work and await its result from the scoped task.
