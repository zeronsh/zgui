//! Why things ran, for devtools (the `trace` feature): per frame, which
//! signals were written and which effects that queued ran, how long each
//! took, and what each is (a node's binding, a script's effect).
//!
//! Built without the feature, none of this exists. Built with it, it does
//! nothing until [`set_enabled`]: a relaxed load on each write and run, no
//! allocation and no bookkeeping.
use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};

static ON: AtomicBool = AtomicBool::new(false);

/// Frames kept, the newest last.
const KEPT: usize = 120;

/// Whether runs are being traced.
#[inline(always)]
pub fn enabled() -> bool {
    ON.load(Ordering::Relaxed)
}
/// Start or stop tracing (on this thread's runtimes). Off, what was
/// recorded is dropped.
pub fn set_enabled(on: bool) {
    ON.store(on, Ordering::Relaxed);
    if !on {
        LOG.with_borrow_mut(|log| *log = Log::default());
    }
}

/// An effect's run in a frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    /// The effect's number.
    pub effect: u64,
    /// What it is, if its maker said (`name`): `@12 binding`, a script line.
    pub name: Option<Rc<str>>,
    /// The signal whose write queued it (its address, `Signal::addr`); none
    /// for a first run, or one queued otherwise.
    pub cause: Option<usize>,
    /// How long it took, in microseconds (what it ran inside included).
    pub micros: f32,
}
/// One frame's work: writes and the runs they caused, in order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Frame {
    /// Its number, counting frames traced.
    pub index: u64,
    /// Signals written (their addresses) and how many times each.
    pub writes: Vec<(usize, u32)>,
    /// Effects run, in the order they ran.
    pub runs: Vec<Run>,
}

#[derive(Default)]
struct Log {
    index: u64,
    current: Frame,
    frames: VecDeque<Frame>,
    causes: HashMap<u64, usize>,
    names: HashMap<u64, Rc<str>>,
}
thread_local! {
    static LOG: RefCell<Log> = RefCell::default();
}

/// Name effect `effect` for traces (`@12 binding`, `app.luau:30 effect`).
pub fn name(effect: u64, name: impl Into<Rc<str>>) {
    if enabled() {
        LOG.with_borrow_mut(|log| {
            log.names.insert(effect, name.into());
        });
    }
}
/// Signal `signal` was written, queueing `queued`.
pub(crate) fn wrote(signal: usize, queued: &[u64]) {
    LOG.with_borrow_mut(|log| {
        match log.current.writes.iter_mut().find(|(s, _)| *s == signal) {
            Some((_, n)) => *n += 1,
            None => log.current.writes.push((signal, 1)),
        }
        for id in queued {
            log.causes.entry(*id).or_insert(signal);
        }
    });
}
/// Effect `effect` ran, from `started` until now.
pub(crate) fn ran(effect: u64, started: Instant) {
    let micros = started.elapsed().as_secs_f32() * 1e6;
    LOG.with_borrow_mut(|log| {
        let cause = log.causes.remove(&effect);
        let name = log.names.get(&effect).cloned();
        log.current.runs.push(Run {
            effect,
            name,
            cause,
            micros,
        });
    });
}
/// The frame being traced is done: kept if anything happened in it.
pub fn end_frame() {
    if !enabled() {
        return;
    }
    LOG.with_borrow_mut(|log| {
        log.index += 1;
        if log.current.writes.is_empty() && log.current.runs.is_empty() {
            return;
        }
        let mut frame = std::mem::take(&mut log.current);
        frame.index = log.index;
        log.frames.push_back(frame);
        while log.frames.len() > KEPT {
            log.frames.pop_front();
        }
    });
}
/// The last `n` frames that did anything, oldest first.
pub fn frames(n: usize) -> Vec<Frame> {
    LOG.with_borrow(|log| {
        let skip = log.frames.len().saturating_sub(n);
        log.frames.iter().skip(skip).cloned().collect()
    })
}
