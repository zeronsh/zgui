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
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    time::Instant,
};

static ON: AtomicBool = AtomicBool::new(false);

/// Frames kept, the newest last: four seconds at 120 Hz (a fling's), until
/// [`set_kept`] says otherwise.
static KEPT: AtomicUsize = AtomicUsize::new(DEFAULT_KEPT);
/// Frames kept unless said otherwise.
pub const DEFAULT_KEPT: usize = 480;
/// The most frames kept: about three minutes at 120 Hz.
pub const MOST_KEPT: usize = 20_000;

/// Whether runs are being traced.
#[inline(always)]
pub fn enabled() -> bool {
    ON.load(Ordering::Relaxed)
}
/// Keep the last `frames` frames (at least one, at most [`MOST_KEPT`]): a
/// few flings outrun the default. Older ones go at once.
pub fn set_kept(frames: usize) {
    let frames = frames.clamp(1, MOST_KEPT);
    KEPT.store(frames, Ordering::Relaxed);
    LOG.with_borrow_mut(|log| {
        while log.frames.len() > frames {
            log.frames.pop_front();
        }
    });
}
/// How many frames are kept ([`set_kept`]).
pub fn kept() -> usize {
    KEPT.load(Ordering::Relaxed)
}
/// Start or stop tracing (on this thread's runtimes), phases measured with
/// it (`zgx::profile::set_recording`). Off, what was recorded is dropped.
pub fn set_enabled(on: bool) {
    ON.store(on, Ordering::Relaxed);
    crate::profile::set_recording(on);
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
/// One frame's work: writes and the runs they caused, in order, and what
/// each phase of it took.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Frame {
    /// Its number, counting frames traced.
    pub index: u64,
    /// Signals written (their addresses) and how many times each.
    pub writes: Vec<(usize, u32)>,
    /// Effects run, in the order they ran.
    pub runs: Vec<Run>,
    /// Each phase that ran (`zgx::profile`'s: input, script, update, style,
    /// layout, text, damage, paint, encode, submit, present): its name,
    /// instructions (0 where not counted) and microseconds.
    pub phases: Vec<(&'static str, u64, f32)>,
    /// When it ended (was shown), in milliseconds since tracing began
    /// ([`now_ms`]'s clock): the time between frames is their pacing.
    pub ended_ms: f64,
    /// Why it did what it did, where the engine said ([`note`]): a scroll
    /// repainted rather than copied, a wait for a later refresh, input that
    /// waited for the window to present again.
    pub notes: Vec<Rc<str>>,
    /// How long the GPU itself ran the frame, in microseconds, where the
    /// renderer measures it ([`gpu`]; macOS's direct Metal frames).
    pub gpu_micros: Option<f32>,
}

#[derive(Default)]
struct Log {
    /// When tracing began: frames' times count from it.
    began: Option<Instant>,
    index: u64,
    current: Frame,
    frames: VecDeque<Frame>,
    causes: HashMap<u64, usize>,
    names: HashMap<u64, Rc<str>>,
}
thread_local! {
    static LOG: RefCell<Log> = RefCell::default();
}

/// The frame log's lines of a memory report: its frames, and the names of
/// the effects it can tell apart.
pub fn memory(report: &mut crate::memory::Report) {
    LOG.with_borrow(|log| {
        let frame = |f: &Frame| {
            size_of::<Frame>()
                + f.writes.capacity() * size_of::<(usize, u32)>()
                + f.runs.capacity() * size_of::<Run>()
                + f.phases.capacity() * size_of::<(&'static str, u64, f32)>()
                + f.notes.capacity() * size_of::<Rc<str>>()
        };
        let frames: usize = log.frames.iter().map(frame).sum();
        report.add(
            "devtools",
            "frame log",
            Some(log.frames.len()),
            Some(frames),
        );
        let names = log.names.values().map(|name| name.len() + 16).sum();
        report.add(
            "devtools",
            "effect names",
            Some(log.names.len()),
            Some(names),
        );
    });
}

/// Name effect `effect` for traces (`@12 binding`, `app.luau:30 effect`).
pub fn name(effect: u64, name: impl Into<Rc<str>>) {
    if enabled() {
        LOG.with_borrow_mut(|log| {
            log.names.insert(effect, name.into());
        });
    }
}
/// Effect `effect` is gone: its name, and the cause of a run it was queued
/// for and never made, go too (each mount names its effects anew).
pub(crate) fn forget(effect: u64) {
    // Dropped as the thread ends, the log may be gone already.
    let _ = LOG.try_with(|log| {
        if let Ok(mut log) = log.try_borrow_mut() {
            log.names.remove(&effect);
            log.causes.remove(&effect);
        }
    });
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
/// Why the frame being traced did something (`zgx::profile::note`, which
/// builds the text only while tracing). A note given twice in a frame is kept
/// once.
pub fn note(why: String) {
    LOG.with_borrow_mut(|log| {
        if !log.current.notes.iter().any(|n| **n == *why) {
            log.current.notes.push(why.into());
        }
    });
}
/// The GPU's own time for the frame being traced (`zgx::profile::gpu_time`).
pub fn gpu(time: std::time::Duration) {
    LOG.with_borrow_mut(|log| {
        log.current.gpu_micros = Some(time.as_secs_f32() * 1e6);
    });
}
/// The frame being traced is done (`zgx::profile::frame`, as a renderer
/// presents it), its phases' split given: kept if anything happened in it.
pub fn end_frame(phases: Vec<(&'static str, u64, f32)>) {
    if !enabled() {
        return;
    }
    LOG.with_borrow_mut(|log| {
        log.index += 1;
        if log.current.writes.is_empty()
            && log.current.runs.is_empty()
            && log.current.notes.is_empty()
            && phases.is_empty()
        {
            return;
        }
        let mut frame = std::mem::take(&mut log.current);
        frame.phases = phases;
        frame.index = log.index;
        let began = *log.began.get_or_insert_with(Instant::now);
        frame.ended_ms = began.elapsed().as_secs_f64() * 1e3;
        log.frames.push_back(frame);
        while log.frames.len() > kept() {
            log.frames.pop_front();
        }
    });
}
/// Now, on frames' clock (`Frame::ended_ms`): milliseconds since tracing
/// began.
pub fn now_ms() -> f64 {
    LOG.with_borrow_mut(|log| {
        log.began
            .get_or_insert_with(Instant::now)
            .elapsed()
            .as_secs_f64()
            * 1e3
    })
}
/// The number of the last frame traced (each frame's `index` counts from
/// here): a mark to read frames after.
pub fn frame_index() -> u64 {
    LOG.with_borrow(|log| log.index)
}
/// The last `n` frames that did anything, oldest first.
pub fn frames(n: usize) -> Vec<Frame> {
    LOG.with_borrow(|log| {
        let skip = log.frames.len().saturating_sub(n);
        log.frames.iter().skip(skip).cloned().collect()
    })
}

#[cfg(test)]
mod tests {
    use crate::{compose::prelude::*, widgets::Ui};

    fn sizes() -> (usize, usize) {
        super::LOG.with_borrow(|log| (log.names.len(), log.causes.len()))
    }

    #[test]
    fn remounting_while_tracing_keeps_its_names_bounded() {
        super::set_enabled(true);
        let mut ui = Ui::new(200., 200.);
        let count = ui.signal(0u32);
        let shown = ui.signal(true);
        let (read, show) = (count.clone(), shown.clone());
        ui.mount(column().child(switch(
            move || show.get(),
            move |on, _| {
                let read = read.clone();
                column().children((0..if on { 20 } else { 0 }).map(move |i| {
                    let read = read.clone();
                    text_signal(move || format!("{i}: {}", read.get()))
                }))
            },
        )));
        ui.prepare_frame();
        let once = sizes();
        for round in 0..30 {
            // Queued, then taken away before it runs: its cause goes too.
            count.set(round + 1);
            shown.set(false);
            ui.prepare_frame();
            shown.set(true);
            ui.prepare_frame();
        }
        let after = sizes();
        super::set_enabled(false);
        assert!(once.0 > 0, "the mount named its effects");
        assert!(
            after.0 <= once.0 + 4 && after.1 <= once.1 + 4,
            "names and causes stay as many as the mounted effects: {once:?} then {after:?}"
        );
    }
}
