//! The VM's interrupt, called by Luau at every loop back edge and call: the
//! script time budget and garbage collected near the memory limit.
//!
//! mlua's own interrupt hook (`Lua::set_interrupt`) guards each call as a
//! callback: 109 instructions an iteration of every script loop, more than
//! the loop itself. This one is a plain C hook counting in the Lua thread
//! (38), doing its work at one call in 64 as the closure did. It is the
//! crate's unsafe uses of Luau's C API, with `data::sort_by_keys`.
use crate::Deadline;
use mlua::{Lua, ffi};
use std::{
    cell::{Cell, RefCell},
    ffi::{c_int, c_void},
    rc::{Rc, Weak},
};

/// What a VM's interrupt checks.
struct Watch {
    /// The running call's time budget, if any (`Vm::guard`).
    deadline: Rc<Cell<Option<Deadline>>>,
    /// The VM's memory limit in bytes (0: none).
    limit: usize,
    /// Collect once the VM holds this much: halfway from what was live at
    /// the last collection to the limit. Luau collects as the VM allocates,
    /// but builtins such as `table.clone` allocate without giving the
    /// collector a turn, and a loop of them between host calls would fill
    /// the VM with garbage until the limit failed the script.
    collect_at: Cell<usize>,
}

thread_local! {
    /// Each VM's watch, by its main thread. Few VMs run on a thread (the
    /// app's and its plugins'), so a list serves.
    static WATCHES: RefCell<Vec<(usize, Weak<Watch>)>> = const { RefCell::new(Vec::new()) };
}

/// The interrupt installed on a VM; dropped, the VM is no longer watched.
pub(crate) struct Installed {
    main: usize,
    _watch: Rc<Watch>,
}
impl Drop for Installed {
    fn drop(&mut self) {
        let _ = WATCHES.try_with(|w| {
            w.borrow_mut()
                .retain(|(main, watch)| *main != self.main && watch.strong_count() > 0)
        });
    }
}

/// Install the interrupt on `lua`, watching `deadline` and `limit`.
pub(crate) fn install(lua: &Lua, deadline: Rc<Cell<Option<Deadline>>>, limit: usize) -> Installed {
    let watch = Rc::new(Watch {
        deadline,
        limit,
        collect_at: Cell::new(limit / 2),
    });
    let main = Cell::new(0usize);
    // SAFETY: `exec_raw` hands the closure the VM's state, valid while it
    // runs. `lua_mainthread` and `lua_callbacks` only read and write the
    // VM's own global state; setting `interrupt` is how Luau takes a hook.
    let _ = unsafe {
        lua.exec_raw::<()>((), |state| {
            let main_thread = ffi::lua_mainthread(state);
            main.set(main_thread as usize);
            (*ffi::lua_callbacks(main_thread)).interrupt = Some(interrupt);
        })
    };
    let main = main.get();
    WATCHES.with(|w| {
        let mut w = w.borrow_mut();
        w.retain(|(_, watch)| watch.strong_count() > 0);
        w.push((main, Rc::downgrade(&watch)));
    });
    Installed {
        main,
        _watch: watch,
    }
}

/// What a check finds the script should do.
enum Verdict {
    Continue,
    OutOfTime,
}

/// The hook Luau calls. The common case is a counter, kept apart from the
/// check so it saves no registers.
///
/// SAFETY (for Luau): it never unwinds a Rust panic, and it raises an error
/// only from a VM safepoint (`gc < 0`), inside the protected call every
/// script runs in, with nothing left to drop in this frame.
unsafe extern "C-unwind" fn interrupt(state: *mut ffi::lua_State, gc: c_int) {
    // A call from the collector itself: not a point to raise an error at
    // (mlua's hook ignored these too).
    if gc >= 0 {
        return;
    }
    // Interrupts since the last check, counted in the running Lua thread's
    // own word (`lua_getthreaddata`, which nothing else uses): cheaper than
    // a thread local, and a loop always counts on its own thread.
    // SAFETY: reads and writes the running thread's data word.
    let ticks = unsafe { ffi::lua_getthreaddata(state) } as usize + 1;
    unsafe { ffi::lua_setthreaddata(state, ticks as *mut c_void) };
    if ticks.is_multiple_of(64) {
        // SAFETY: as for this hook.
        unsafe { every_64th(state) }
    }
}

/// One interrupt in 64: check, and raise the error when out of time.
///
/// SAFETY: as for `interrupt`.
#[cold]
#[inline(never)]
unsafe extern "C-unwind" fn every_64th(state: *mut ffi::lua_State) {
    // SAFETY: `state` is the running thread of a VM Luau is executing.
    let verdict = unsafe { check(state) };
    if let Verdict::OutOfTime = verdict {
        // Out of time, the next interrupt checks again: a script catching
        // the error with `pcall` meets it at its next loop or call, rather
        // than looping until a check lands inside the `pcall` again.
        // SAFETY: as above.
        unsafe { ffi::lua_setthreaddata(state, 63 as *mut c_void) };
        // SAFETY: pushes a string and raises it as the error; Luau unwinds
        // to the protected call around the script (`lua_error` does not
        // return). Nothing in this frame needs dropping.
        unsafe {
            ffi::lua_pushstring(state, c"script exceeded its time budget".as_ptr());
            ffi::lua_error(state);
        }
    }
}

/// The check at one interrupt in 64: memory, then the clock.
///
/// SAFETY: `state` is the running thread of a VM Luau is executing.
unsafe fn check(state: *mut ffi::lua_State) -> Verdict {
    // SAFETY: reads the thread's VM's main thread.
    let main = unsafe { ffi::lua_mainthread(state) } as usize;
    let watch = WATCHES.with(|w| {
        w.borrow()
            .iter()
            .find(|(m, _)| *m == main)
            .and_then(|(_, watch)| watch.upgrade())
    });
    let Some(watch) = watch else {
        return Verdict::Continue;
    };
    if watch.limit > 0 {
        // SAFETY: the counts and a full collection from a safepoint, as the
        // allocator's own collection steps run at.
        let used = |state| unsafe {
            ffi::lua_gc(state, ffi::LUA_GCCOUNT, 0) as usize * 1024
                + ffi::lua_gc(state, ffi::LUA_GCCOUNTB, 0) as usize
        };
        if used(state) > watch.collect_at.get() {
            unsafe { ffi::lua_gc(state, ffi::LUA_GCCOLLECT, 0) };
            let live = used(state);
            watch
                .collect_at
                .set(live + watch.limit.saturating_sub(live) / 2);
        }
    }
    match watch.deadline.get() {
        Some(d) if d.exceeded() => Verdict::OutOfTime,
        _ => Verdict::Continue,
    }
}
