//! The VM's interrupt, called by Luau at every loop back edge and call while
//! it is set: the script time budget, and garbage collected near the memory
//! limit.
//!
//! It is set only when there is something to do. A watchdog thread sets it
//! once a call's wall time passes its budget (`Alarm::due`), and the VM's
//! allocator once it holds more than halfway to its limit (`collect_at`,
//! through vendored mlua's `set_memory_alarm`). Until then a script loop
//! runs with no hook at all: with one counting every back edge (38
//! instructions, against a floor of 91 an iteration), a filter over a mail
//! app's 4,000 messages spent a tenth of its time in it. Set, the hook
//! counts in the Lua thread and checks at one call in 64, as it always did;
//! with nothing left to do (no budget passed, nothing to collect) it clears
//! itself.
//!
//! Setting the interrupt from the watchdog thread while the VM runs is what
//! Luau's `lua_Callbacks` documents as safe ("interrupt is safe to set from
//! an arbitrary thread"): the VM reads the slot afresh at every safepoint,
//! and a pointer-sized aligned store is atomic on every platform Luau
//! supports. Every write here is an atomic store to the slot (`Alarm::set`),
//! and the watchdog writes only under its lock, which a VM going away takes
//! before mlua clears the slot as it closes (`Installed::drop`).
//!
//! The watchdog costs nothing while no script runs: it sleeps until the
//! earliest call's budget passes and parks with no call running, woken only
//! by a call due before it would wake (`Watchdog::wake_by`). It hears of
//! outermost calls alone: an outermost call's start is an atomic store and
//! load, its end a store, and a burst of short calls wakes it about once a
//! budget. Nested calls and work off the clock only move the budget's end
//! later, so the interrupt may be set early, which only checks it sooner.
//! The budget itself is checked as before (`Deadline::exceeded`, the CPU
//! time from a reading up to 1 ms old), from the moment its wall time
//! passes: a runaway is stopped within its budget plus the watchdog's
//! wake-up and 64 interrupts.
//!
//! The watchdog sets a call's interrupt once, marking its due time fired
//! (`FIRED`), and sleeps on to the next call due. The interrupt then stays
//! set until the call ends: the check clears it only while the call's due
//! time is not marked, and looks again after clearing it, so a watchdog
//! marking the call meanwhile either is seen or sets it after the clearing
//! (all sequentially consistent, `check`).
//!
//! It is one of the crate's unsafe uses of Luau's C API, with
//! `data::sort_by_keys` and `value`'s table conversions. How a Luau error
//! leaves Rust frames, which all of them rely on: Luau is built as C++ with
//! exceptions (`luau0-src` leaves `LUA_USE_LONGJMP` unset), and mlua
//! declares its C API `extern "C-unwind"`, so an error (`lua_error`, a
//! failed allocation) is a C++ exception that unwinds through Rust frames
//! running their drops, to the `lua_pcall` that every script call and
//! `Lua::exec_raw` closure runs inside. A Rust panic must not reach Luau's
//! frames: it would unwind past `lua_pcall` without restoring the VM's call
//! state.
use crate::Deadline;
use mlua::{Lua, ffi};
use std::{
    cell::{Cell, RefCell},
    ffi::{c_int, c_void},
    rc::{Rc, Weak},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicPtr, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

/// Luau's interrupt slot as Rust writes it: the same size and alignment as
/// the function pointer it holds (`lua_Callbacks::interrupt`).
type Slot = AtomicPtr<c_void>;
const _: () = assert!(
    size_of::<Slot>()
        == size_of::<Option<unsafe extern "C-unwind" fn(*mut ffi::lua_State, c_int)>>()
        && align_of::<Slot>()
            == align_of::<Option<unsafe extern "C-unwind" fn(*mut ffi::lua_State, c_int)>>()
);

/// What a VM's interrupt checks.
pub(crate) struct Watch {
    /// The running call's time budget, if any (`Vm::guard`).
    deadline: Rc<Cell<Option<Deadline>>>,
    /// The VM's main thread, for its memory alarm.
    main: *mut ffi::lua_State,
    /// The VM is still watched: `Installed` has not dropped, so `main` is
    /// alive and its memory alarm may point here.
    alive: Cell<bool>,
    /// Shared with the watchdog: the slot and the running call's due time.
    alarm: Arc<Alarm>,
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
    /// The allocator passed `collect_at` (its alarm, `on_memory`): the
    /// interrupt collects at its next check.
    collect: Cell<bool>,
}
impl Watch {
    /// The VM may hold `limit` from now: it collects halfway there.
    pub(crate) fn set_limit(&self, limit: usize) {
        let live = self.live.get();
        self.limit.set(limit);
        self.collect_at.set(live + limit.saturating_sub(live) / 2);
        self.set_memory_alarm();
    }
    /// The allocator's alarm at `collect_at`, if there is a limit.
    fn set_memory_alarm(&self) {
        // A watch kept past its VM (by a meter) sets nothing.
        if !self.alive.get() {
            return;
        }
        let alarm = (self.limit.get() > 0).then_some(mlua::MemoryAlarm {
            call: on_memory,
            data: std::ptr::from_ref(self).cast(),
        });
        // SAFETY: `main` is the VM's main thread, alive while `alive` is:
        // `Installed` clears it as it drops, before the VM goes. Called on
        // its thread, never while it allocates. `data` points at this
        // watch, which the alarm outlives only by never being called: it is
        // set again, or the VM goes, before the watch does (`Installed`
        // owns it, and clears the alarm as it drops).
        unsafe { mlua::set_memory_alarm(self.main, self.collect_at.get(), alarm) };
    }
}

/// The allocator's alarm past `collect_at`: collect at the next interrupt.
/// It runs inside an allocation: it only marks the watch and sets the slot.
///
/// # Safety
///
/// `data` is the `Watch` that set the alarm (`Watch::set_memory_alarm`),
/// alive while the alarm is set; called on the VM's thread.
unsafe fn on_memory(data: *const c_void) {
    // SAFETY: as above.
    let watch = unsafe { &*data.cast::<Watch>() };
    watch.collect.set(true);
    watch.alarm.set(true);
}

/// A VM's alarm, shared with the watchdog thread.
pub(crate) struct Alarm {
    /// Luau's interrupt slot in the VM's global state; null once the VM is
    /// going (`Installed::drop`, under the watchdog's lock).
    slot: AtomicPtr<Slot>,
    /// When the running call's wall time passes its budget, in nanoseconds
    /// from `epoch`, with `FIRED` once the watchdog has set the interrupt
    /// for it; 0 while no call runs.
    due: AtomicU64,
}
/// Marks a due time the watchdog has set the interrupt for (`Alarm::due`;
/// nanoseconds from `epoch` reach it after 292 years).
const FIRED: u64 = 1 << 63;
impl Alarm {
    /// The watchdog set the interrupt for the running call: it stays set.
    fn fired(&self) -> bool {
        self.due.load(Ordering::SeqCst) & FIRED != 0
    }
    /// Set the interrupt (or clear it): an atomic store, as Luau allows
    /// from any thread.
    fn set(&self, on: bool) {
        let slot = self.slot.load(Ordering::Acquire);
        if slot.is_null() {
            return;
        }
        let hook = if on {
            interrupt as unsafe extern "C-unwind" fn(*mut ffi::lua_State, c_int) as *mut c_void
        } else {
            std::ptr::null_mut()
        };
        // SAFETY: `slot` points at the interrupt slot of a VM that is alive:
        // it is nulled before the VM goes, under the watchdog's lock, which
        // the watchdog holds while it sets the slot; the VM's own thread
        // sets it only while the VM is alive. (Already so, left alone: a
        // store to the VM's callbacks is not free, and calls end often.)
        // The load too is sequentially consistent with `Alarm::due`, for
        // `check`'s clearing: the watchdog's, after its mark, sees a
        // clearing ordered before the mark rather than the hook it cleared.
        let slot = unsafe { &*slot };
        if slot.load(Ordering::SeqCst) != hook {
            slot.store(hook, Ordering::SeqCst);
        }
    }
}

/// When the watchdog's clock started: due times count from here.
fn epoch() -> Instant {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    *EPOCH.get_or_init(Instant::now)
}
/// `at` in nanoseconds from `epoch` (at least 1: 0 is no call).
#[inline]
fn nanos(at: Instant) -> u64 {
    let since = at.saturating_duration_since(epoch());
    (since.as_secs() * 1_000_000_000 + u64::from(since.subsec_nanos())).max(1)
}

/// The thread that sets the interrupt of VMs whose call ran past its wall
/// budget, one for the process.
struct Watchdog {
    alarms: Mutex<Vec<std::sync::Weak<Alarm>>>,
    /// Its thread, set once it is spawned (until then, it has not parked).
    thread: OnceLock<std::thread::Thread>,
    /// When it next wakes by itself (nanoseconds from `epoch`; `u64::MAX`
    /// while parked for good): a call due before then wakes it.
    sleep_until: AtomicU64,
}
/// How long before a call's budget passes the watchdog sets its interrupt:
/// a thread's timed wake-up can come late by a few milliseconds (the system
/// coalesces timers), and the check then stops the script as soon as its
/// budget is spent. Only a call running almost its whole budget runs its
/// last moments with the interrupt set.
const EARLY: u64 = 5_000_000;
impl Watchdog {
    /// The process's watchdog.
    fn get() -> &'static Watchdog {
        static WATCHDOG: OnceLock<&'static Watchdog> = OnceLock::new();
        WATCHDOG.get_or_init(Watchdog::spawn)
    }
    /// A watchdog of its own, on its own thread (the process's, or a test's).
    fn spawn() -> &'static Watchdog {
        epoch();
        let watchdog: &'static Watchdog = Box::leak(Box::new(Watchdog {
            alarms: Mutex::new(Vec::new()),
            thread: OnceLock::new(),
            sleep_until: AtomicU64::new(u64::MAX),
        }));
        let thread = std::thread::Builder::new()
            .name("zgx script watchdog".into())
            .spawn(move || watchdog.run())
            .expect("the script watchdog thread");
        let _ = watchdog.thread.set(thread.thread().clone());
        watchdog
    }
    /// A call due at `due` started: wake if that is before the next wake.
    /// With `run`'s check after it publishes when it sleeps until, one of
    /// the two sees the other (both sequentially consistent).
    fn wake_by(&self, due: u64) {
        if due < self.sleep_until.load(Ordering::SeqCst)
            && let Some(thread) = self.thread.get()
        {
            thread.unpark();
        }
    }
    fn run(&self) {
        loop {
            let next = self.scan();
            self.sleep_until.store(next, Ordering::SeqCst);
            // A call due before then, published while scanning, is seen
            // here, or sees `sleep_until` and unparks (`wake_by`).
            if self.earliest() < next {
                continue;
            }
            match next {
                u64::MAX => std::thread::park(),
                next => {
                    let now = nanos(Instant::now());
                    if next > now {
                        std::thread::park_timeout(Duration::from_nanos(next - now));
                    }
                }
            }
        }
    }
    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<std::sync::Weak<Alarm>>> {
        self.alarms.lock().unwrap_or_else(|e| e.into_inner())
    }
    /// Set the interrupt of VMs past their call's budget, once a call
    /// (marking it `FIRED`); when to look next.
    fn scan(&self) -> u64 {
        let now = nanos(Instant::now());
        let mut alarms = self.lock();
        alarms.retain(|alarm| alarm.strong_count() > 0);
        let mut next = u64::MAX;
        for alarm in alarms.iter().filter_map(std::sync::Weak::upgrade) {
            match alarm.due.load(Ordering::SeqCst) {
                due if due == 0 || due & FIRED != 0 => {}
                // Marked first, then set: a check clearing the interrupt
                // meanwhile sees the mark, or the setting follows its
                // clearing (`check`). A call ended or begun since is left
                // to the next scan (`earliest`); the interrupt set as one
                // ends only checks once more.
                due if due <= now => {
                    let fired = alarm.due.compare_exchange(
                        due,
                        due | FIRED,
                        Ordering::SeqCst,
                        Ordering::SeqCst,
                    );
                    if fired.is_ok() {
                        alarm.set(true);
                    }
                }
                due => next = next.min(due),
            }
        }
        next
    }
    /// The earliest call due and not yet fired.
    fn earliest(&self) -> u64 {
        self.lock()
            .iter()
            .filter_map(std::sync::Weak::upgrade)
            .map(|alarm| alarm.due.load(Ordering::SeqCst))
            .filter(|due| *due != 0 && due & FIRED == 0)
            .min()
            .unwrap_or(u64::MAX)
    }
}

thread_local! {
    /// Each VM's watch, by its main thread. Few VMs run on a thread (the
    /// app's and its plugins'), so a list serves.
    static WATCHES: RefCell<Vec<(usize, Weak<Watch>)>> = const { RefCell::new(Vec::new()) };
}

/// The interrupt and alarms installed on a VM; dropped, the VM is no
/// longer watched.
pub(crate) struct Installed {
    main: usize,
    watch: Rc<Watch>,
    watchdog: &'static Watchdog,
}
impl Installed {
    /// What the interrupt checks, to change the limit it collects by.
    pub(crate) fn watch(&self) -> Rc<Watch> {
        self.watch.clone()
    }
    /// Whether the watchdog counts a call running, and the interrupt set.
    #[cfg(test)]
    pub(crate) fn state(&self) -> (bool, bool) {
        let slot = self.watch.alarm.slot.load(Ordering::Acquire);
        // SAFETY: the VM is alive while this is (`Drop`).
        let set = !slot.is_null() && unsafe { !(*slot).load(Ordering::Acquire).is_null() };
        (self.watch.alarm.due.load(Ordering::SeqCst) != 0, set)
    }
    /// An outermost call began, with `deadline`, or ended (`None`): tell
    /// the watchdog when it is due. With no call running and nothing to
    /// collect, the interrupt has nothing to do.
    #[inline]
    pub(crate) fn publish(&self, deadline: Option<Deadline>) {
        let alarm = &self.watch.alarm;
        match deadline {
            Some(deadline) => {
                // Set early (`EARLY`): the check decides when the budget is
                // spent, so the stop stays as prompt as the check's.
                let due = nanos(deadline.due()).saturating_sub(EARLY).max(1);
                alarm.due.store(due, Ordering::SeqCst);
                self.watchdog.wake_by(due);
            }
            None => {
                alarm.due.store(0, Ordering::SeqCst);
                if !self.watch.collect.get() {
                    alarm.set(false);
                }
            }
        }
    }
}
impl Drop for Installed {
    fn drop(&mut self) {
        // The watchdog sets slots under its lock: nulled under it, this one
        // is not set again, before mlua clears the slot as the VM closes.
        {
            let mut alarms = self.watchdog.lock();
            self.watch
                .alarm
                .slot
                .store(std::ptr::null_mut(), Ordering::Release);
            alarms.retain(|alarm| alarm.strong_count() > 0);
        }
        // SAFETY: the VM is alive (it drops after this) and not allocating.
        unsafe { mlua::set_memory_alarm(self.watch.main, 0, None) };
        self.watch.alive.set(false);
        let _ = WATCHES.try_with(|w| {
            w.borrow_mut()
                .retain(|(main, watch)| *main != self.main && watch.strong_count() > 0)
        });
    }
}

/// Watch `lua`: its interrupt, set when `deadline` passes or garbage needs
/// collecting before `limit`.
pub(crate) fn install(lua: &Lua, deadline: Rc<Cell<Option<Deadline>>>, limit: usize) -> Installed {
    install_on(Watchdog::get(), lua, deadline, limit)
}

/// `install`, watched by `watchdog`.
fn install_on(
    watchdog: &'static Watchdog,
    lua: &Lua,
    deadline: Rc<Cell<Option<Deadline>>>,
    limit: usize,
) -> Installed {
    let (main, slot) = (
        Cell::new(std::ptr::null_mut()),
        Cell::new(std::ptr::null_mut()),
    );
    // SAFETY: `exec_raw` hands the closure the VM's state, valid while it
    // runs. `lua_mainthread` and `lua_callbacks` only read the VM's global
    // state, and `lua_callbacks` points into it, live as long as the VM:
    // the interrupt's slot there is written only through `Alarm::set` until
    // mlua clears it as the VM closes. Nothing here raises an error.
    let _ = unsafe {
        lua.exec_raw::<()>((), |state| {
            let main_thread = ffi::lua_mainthread(state);
            main.set(main_thread);
            let callbacks = ffi::lua_callbacks(main_thread);
            slot.set(std::ptr::addr_of_mut!((*callbacks).interrupt).cast::<Slot>());
        })
    };
    let alarm = Arc::new(Alarm {
        slot: AtomicPtr::new(slot.get()),
        due: AtomicU64::new(0),
    });
    let watch = Rc::new(Watch {
        deadline,
        main: main.get(),
        alive: Cell::new(true),
        alarm: alarm.clone(),
        limit: Cell::new(limit),
        live: Cell::new(0),
        collect_at: Cell::new(limit / 2),
        collect: Cell::new(false),
    });
    watch.set_memory_alarm();
    watchdog.lock().push(Arc::downgrade(&alarm));
    let main = main.get() as usize;
    WATCHES.with(|w| {
        let mut w = w.borrow_mut();
        w.retain(|(_, watch)| watch.strong_count() > 0);
        w.push((main, Rc::downgrade(&watch)));
    });
    Installed {
        main,
        watch,
        watchdog,
    }
}

/// What a check finds the script should do.
enum Verdict {
    Continue,
    OutOfTime,
}

/// The hook Luau calls while it is set. The common case is a counter, kept
/// apart from the check so it saves no registers.
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

/// The check at one interrupt in 64: garbage the allocator asked to have
/// collected, then the clock; with neither to do, the interrupt clears
/// itself. It must not panic (see the module): its thread local is read
/// with `try_with` (a VM can run while the thread's locals are being
/// destroyed) and borrowed only here and in `install` and `Installed::drop`,
/// none of which runs a script.
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
    if watch.collect.replace(false) && limit > 0 {
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
        unsafe { ffi::lua_gc(state, ffi::LUA_GCCOLLECT, 0) };
        let live = used(state);
        watch.live.set(live);
        watch.collect_at.set(live + limit.saturating_sub(live) / 2);
        watch.set_memory_alarm();
    }
    match watch.deadline.get() {
        Some(d) if d.exceeded() => Verdict::OutOfTime,
        // Nothing to collect and the call not yet fired: until the watchdog
        // or the allocator sets it again, loops run without it. Fired (a
        // nested call or work off the clock may have moved the budget's end
        // later since), it stays set until the call ends, and the check
        // (`Deadline::exceeded`, on the budget as it is) decides.
        _ if !watch.collect.get() && !watch.alarm.fired() => {
            // Then looked at again: a watchdog marking the call after the
            // look above is seen here, or else its mark, and so its setting,
            // follows this clearing in the order all of them share.
            watch.alarm.set(false);
            if watch.alarm.fired() {
                watch.alarm.set(true);
            }
            Verdict::Continue
        }
        _ => Verdict::Continue,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The interrupt is set only with something to do: not while calls run
    /// within their budget, but once the VM allocates past `collect_at`,
    /// and it stays set past a call's end until it has collected.
    #[test]
    fn the_interrupt_is_set_by_the_memory_alarm() {
        let lua = Lua::new();
        let limit = 8 << 20;
        lua.set_memory_limit(limit).unwrap();
        let deadline: Rc<Cell<Option<Deadline>>> = Rc::default();
        let installed = install(&lua, deadline, limit);
        let slot = installed.watch.alarm.slot.load(Ordering::Acquire);
        let set = || unsafe { !(*slot).load(Ordering::Acquire).is_null() };
        installed.publish(Some(Deadline::start(Duration::from_secs(60))));
        lua.load("local t = {} for i = 1, 100 do t[i] = i end")
            .exec()
            .unwrap();
        installed.publish(None);
        assert!(!set(), "no hook while calls run within their budget");
        // Past halfway to the limit (kept alive, so nothing frees it).
        lua.load("big = {} for i = 1, 200000 do big[i] = i end")
            .exec()
            .unwrap();
        // The alarm set the interrupt, whose check collected as the loop ran
        // on, then set the alarm again further up.
        assert!(installed.watch.live.get() > limit / 4, "collected");
        assert!(
            installed.watch.collect_at.get() > limit / 2,
            "the alarm set again"
        );
        // An alarm going off as a call ends keeps the interrupt set for it.
        unsafe { on_memory(std::ptr::from_ref(&*installed.watch).cast()) };
        installed.publish(None);
        assert!(set(), "a pending collection keeps it set past a call's end");
    }

    /// With no call running the watchdog parks for good: an idle app wakes
    /// nothing. A call due wakes it, and it sleeps until that call is due.
    #[test]
    fn the_watchdog_parks_while_no_call_runs() {
        let lua = Lua::new();
        let deadline: Rc<Cell<Option<Deadline>>> = Rc::default();
        // A watchdog of its own: other tests' scripts run on the process's.
        let watchdog = Watchdog::spawn();
        let installed = install_on(watchdog, &lua, deadline, 0);
        let settled = |want: &dyn Fn(u64) -> bool| {
            let start = Instant::now();
            while !want(watchdog.sleep_until.load(Ordering::SeqCst)) {
                assert!(
                    start.elapsed() < Duration::from_secs(5),
                    "the watchdog never settled"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
        };
        // Whatever it sleeps until (a call ended keeps it to that call's due
        // time, when it finds none and parks), a call due sooner wakes it.
        let soon = Deadline::start(Duration::from_millis(20));
        installed.publish(Some(soon));
        installed.publish(None);
        settled(&|until| until == u64::MAX);
        let call = Deadline::start(Duration::from_secs(60));
        installed.publish(Some(call));
        // (`EARLY` before its budget passes.)
        let due = nanos(call.due()) - EARLY;
        settled(&|until| until == due);
        installed.publish(None);
        // It wakes at that call's due time (a minute on), finds no call and
        // parks for good: woken early here by a call due sooner.
        let soon = Deadline::start(Duration::from_millis(20));
        installed.publish(Some(soon));
        installed.publish(None);
        settled(&|until| until == u64::MAX);
    }

    /// A call past its budget has its interrupt set once: the watchdog
    /// then parks rather than waking while the call runs on, and the check
    /// leaves the interrupt set until the call ends.
    #[test]
    fn the_watchdog_sets_a_call_past_its_budget_once() {
        let lua = Lua::new();
        let deadline: Rc<Cell<Option<Deadline>>> = Rc::default();
        let watchdog = Watchdog::spawn();
        let installed = install_on(watchdog, &lua, deadline.clone(), 0);
        let call = Deadline::start(Duration::ZERO);
        deadline.set(Some(call));
        installed.publish(Some(call));
        let start = Instant::now();
        while !installed.watch.alarm.fired()
            || watchdog.sleep_until.load(Ordering::SeqCst) != u64::MAX
        {
            assert!(start.elapsed() < Duration::from_secs(5), "never fired");
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(installed.state(), (true, true));
        // Its budget moved later (work off the clock): checks find it not
        // exceeded and leave the interrupt set.
        deadline.set(Some(Deadline::start(Duration::from_secs(60))));
        lua.load("local n = 0 for i = 1, 1000 do n += i end")
            .exec()
            .unwrap();
        assert_eq!(installed.state(), (true, true), "set until the call ends");
        installed.publish(None);
        assert_eq!(installed.state(), (false, false));
    }
}
