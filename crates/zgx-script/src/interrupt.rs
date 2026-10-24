//! The VM's interrupt, called by Luau at every loop back edge and call: the
//! script time budget and garbage collected near the memory limit.
//!
//! mlua's own interrupt hook (`Lua::set_interrupt`) guards each call as a
//! callback: 109 instructions an iteration of every script loop, more than
//! the loop itself. This one is a plain C hook counting in the Lua thread
//! (38), doing its work at one call in 64 as the closure did. It is one of
//! the crate's unsafe uses of Luau's C API, with `data::sort_by_keys` and
//! `value`'s table conversions.
//!
//! How a Luau error leaves Rust frames, which all of them rely on: Luau is
//! built as C++ with exceptions (`luau0-src` leaves `LUA_USE_LONGJMP`
//! unset), and mlua declares its C API `extern "C-unwind"`, so an error
//! (`lua_error`, a failed allocation) is a C++ exception that unwinds
//! through Rust frames running their drops, to the `lua_pcall` that every
//! script call and `Lua::exec_raw` closure runs inside. A Rust panic must
//! not reach Luau's frames: it would unwind past `lua_pcall` without
//! restoring the VM's call state.
use crate::Deadline;
use mlua::{Lua, ffi};
use std::{
    cell::{Cell, RefCell},
    ffi::{c_int, c_void},
    rc::{Rc, Weak},
};

/// What a VM's interrupt checks.
pub(crate) struct Watch {
    /// The running call's time budget, if any (`Vm::guard`).
    deadline: Rc<Cell<Option<Deadline>>>,
    /// The VM's memory limit in bytes (0: none): less, for a plugin, what
    /// its states hold (`meter`).
    limit: Cell<usize>,
    /// What was live at the last collection.
    live: Cell<usize>,
    /// Collect once the VM holds this much: halfway from what was live at
    /// the last collection to the limit. Luau collects as the VM allocates,
    /// but builtins such as `table.clone` allocate without giving the
    /// collector a turn, and a loop of them between host calls would fill
    /// the VM with garbage until the limit failed the script.
    collect_at: Cell<usize>,
}
impl Watch {
    /// The VM may hold `limit` from now: it collects halfway there.
    pub(crate) fn set_limit(&self, limit: usize) {
        let live = self.live.get();
        self.limit.set(limit);
        self.collect_at.set(live + limit.saturating_sub(live) / 2);
    }
}

thread_local! {
    /// Each VM's watch, by its main thread. Few VMs run on a thread (the
    /// app's and its plugins'), so a list serves.
    static WATCHES: RefCell<Vec<(usize, Weak<Watch>)>> = const { RefCell::new(Vec::new()) };
}

/// The interrupt installed on a VM; dropped, the VM is no longer watched.
pub(crate) struct Installed {
    main: usize,
    watch: Rc<Watch>,
}
impl Installed {
    /// What the interrupt checks, to change the limit it collects by.
    pub(crate) fn watch(&self) -> Rc<Watch> {
        self.watch.clone()
    }
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
        limit: Cell::new(limit),
        live: Cell::new(0),
        collect_at: Cell::new(limit / 2),
    });
    let main = Cell::new(0usize);
    // SAFETY: `exec_raw` hands the closure the VM's state, valid while it
    // runs. `lua_mainthread` and `lua_callbacks` only read the VM's global
    // state, and `lua_callbacks` points into it, live as long as the VM;
    // setting `interrupt` is how Luau takes a hook (mlua clears it when the
    // VM closes). Nothing here raises an error.
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
    Installed { main, watch }
}

/// What a check finds the script should do.
enum Verdict {
    Continue,
    OutOfTime,
}

/// The hook Luau calls. The common case is a counter, kept apart from the
/// check so it saves no registers.
///
/// # Safety
///
/// Only Luau calls it, with the running thread's `state`. For Luau's sake
/// it never panics (see the module), and it raises an error only from a VM
/// safepoint (`gc < 0`: a loop back edge, a call or a return, where the VM
/// has saved its frame, `VM_PROTECT`), never from the collector's calls.
/// Script code runs only inside mlua's protected calls, so the error
/// always has a `lua_pcall` (or `lua_resume`) to unwind to.
unsafe extern "C-unwind" fn interrupt(state: *mut ffi::lua_State, gc: c_int) {
    // A call from the collector itself: not a point to raise an error at
    // (mlua's hook ignored these too).
    if gc >= 0 {
        return;
    }
    // Interrupts since the last check, counted in the running Lua thread's
    // own word (`lua_getthreaddata`, which nothing else uses): cheaper than
    // a thread local, and a loop always counts on its own thread. A new
    // thread's word starts at null (Luau's `preinit_state`).
    // SAFETY: reads and writes the running thread's data word, a plain
    // field of `state`: no allocation, no error. The word is never
    // dereferenced, so any value is fine (it wraps rather than panics).
    let ticks = (unsafe { ffi::lua_getthreaddata(state) } as usize).wrapping_add(1);
    unsafe { ffi::lua_setthreaddata(state, ticks as *mut c_void) };
    if ticks.is_multiple_of(64) {
        // SAFETY: as for this hook.
        unsafe { every_64th(state) }
    }
}

/// One interrupt in 64: check, and raise the error when out of time.
///
/// # Safety
///
/// As for `interrupt`, from which alone it is called (`gc < 0`).
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
        // SAFETY: pushes a string and raises it as the error. Luau's push
        // makes room for the slot itself (`ensure_stack`), and whatever it
        // writes over is abandoned by the error. `lua_error` does not
        // return: it unwinds (a C++ exception) through this frame and
        // `interrupt`'s, which hold nothing to drop, to the protected call
        // around the script.
        unsafe {
            ffi::lua_pushstring(state, c"script exceeded its time budget".as_ptr());
            ffi::lua_error(state);
        }
    }
}

/// The check at one interrupt in 64: memory, then the clock. It must not
/// panic (see the module): its thread local is read with `try_with` (a VM
/// can run while the thread's locals are being destroyed) and borrowed only
/// here and in `install` and `Installed::drop`, none of which runs a script.
///
/// # Safety
///
/// `state` is the running thread of a VM Luau is executing, at a safepoint.
unsafe fn check(state: *mut ffi::lua_State) -> Verdict {
    // SAFETY: reads the thread's VM's main thread.
    let main = unsafe { ffi::lua_mainthread(state) } as usize;
    let watch = WATCHES
        .try_with(|w| {
            w.borrow()
                .iter()
                .find(|(m, _)| *m == main)
                .and_then(|(_, watch)| watch.upgrade())
        })
        .ok()
        .flatten();
    let Some(watch) = watch else {
        return Verdict::Continue;
    };
    let limit = watch.limit.get();
    if limit > 0 {
        // SAFETY: the counts read the VM's totals. The full collection runs
        // at a safepoint, where the VM has saved its frame and keeps the
        // thread's top above every live register, as it does for the
        // collection steps its own instructions take. Neither raises an
        // error (a collection only frees and shrinks). The collector may run
        // userdata destructors (Rust drops), as any allocation's step may;
        // no borrow of `WATCHES` is held meanwhile, so one dropping an
        // `Installed` is fine.
        let used = |state| unsafe {
            ffi::lua_gc(state, ffi::LUA_GCCOUNT, 0) as usize * 1024
                + ffi::lua_gc(state, ffi::LUA_GCCOUNTB, 0) as usize
        };
        if used(state) > watch.collect_at.get() {
            unsafe { ffi::lua_gc(state, ffi::LUA_GCCOLLECT, 0) };
            let live = used(state);
            watch.live.set(live);
            watch.collect_at.set(live + limit.saturating_sub(live) / 2);
        }
    }
    match watch.deadline.get() {
        Some(d) if d.exceeded() => Verdict::OutOfTime,
        _ => Verdict::Continue,
    }
}
