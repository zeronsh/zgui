//! Work a host runs once it goes idle (its frames stopped for a moment):
//! what frees memory a busy frame would not stop for. A script host
//! collects its VMs' garbage here: Luau's collector runs only as scripts
//! allocate, and counts only its own heap, so what a script let go of on
//! the Rust side (a node's userdata holds far more than its own bytes)
//! would otherwise stay until the scripts next ran a while.
use crate::reactive::Runtime;
use std::{
    rc::{Rc, Weak},
    sync::atomic::{AtomicBool, Ordering},
};

static WHOLE_RUNS: AtomicBool = AtomicBool::new(false);

/// Says the program's allocator returns memory only in runs larger than a
/// page (zgx-desktop's `mimalloc`), so that what frees a page at a time
/// keeps its pages apart to give them back: a script host's VMs do.
pub fn set_heap_returns_whole_runs() {
    WHOLE_RUNS.store(true, Ordering::Relaxed);
}

/// Whether [`set_heap_returns_whole_runs`] was called.
pub fn heap_returns_whole_runs() -> bool {
    WHOLE_RUNS.load(Ordering::Relaxed)
}

/// Keeps a function registered with [`on_idle`]: dropped, it runs no more.
#[must_use = "dropping it unregisters the function"]
pub struct IdleTrim(#[allow(dead_code)] Rc<dyn Fn()>);

/// Runs `trim` each time a host of `runtime` goes idle, while the returned
/// [`IdleTrim`] lives.
pub fn on_idle(runtime: &Runtime, trim: impl Fn() + 'static) -> IdleTrim {
    let trim: Rc<dyn Fn()> = Rc::new(trim);
    runtime.idle_trims().borrow_mut().push(Rc::downgrade(&trim));
    IdleTrim(trim)
}

/// Runs what was registered with [`on_idle`]: hosts call this once as they
/// go idle (zgx-desktop, as its frames stop), not per frame.
pub fn trim(runtime: &Runtime) {
    let live: Vec<Rc<dyn Fn()>> = {
        let mut trims = runtime.idle_trims().borrow_mut();
        trims.retain(|trim| trim.strong_count() > 0);
        trims.iter().filter_map(Weak::upgrade).collect()
    };
    for trim in live {
        trim();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn a_registered_function_runs_on_each_trim_until_dropped() {
        let runtime = Runtime::new();
        let runs = Rc::new(Cell::new(0));
        let counted = runs.clone();
        let handle = on_idle(&runtime, move || counted.set(counted.get() + 1));
        trim(&runtime);
        trim(&runtime);
        assert_eq!(runs.get(), 2);
        drop(handle);
        trim(&runtime);
        assert_eq!(runs.get(), 2);
    }
}
