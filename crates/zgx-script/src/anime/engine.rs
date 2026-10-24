//! The animation engine. One task ticks every playing clip once per display
//! frame, in one reactive batch; each clip renders its tracks for its time,
//! and every element written this frame is flushed to the scene once.
//! Nothing runs while nothing plays: the task waits without asking for
//! frames. Script code runs per frame only for callbacks a script set
//! (`onUpdate`, `onRender`, function eases or modifiers).
use super::{
    ease::Ease,
    target::{Key, Target},
    value::V,
};
use crate::{Vm, element::Element};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    future::poll_fn,
    rc::{Rc, Weak},
    task::{Poll, Waker},
    time::Instant,
};
use zgx::{frame::FrameClock, reactive::Runtime};

#[derive(Clone)]
pub(crate) enum Modifier {
    /// Round to this many decimals: no script call.
    Round(f64),
    Script(mlua::Function),
}
impl Modifier {
    fn apply(&self, v: V, vm: Option<&Vm>) -> V {
        let V::Num(n) = v else { return v };
        match self {
            Modifier::Round(decimals) => {
                let scale = 10f64.powf(*decimals);
                V::Num((n * scale).round() / scale)
            }
            Modifier::Script(f) => match vm.map(|vm| (vm, vm.call::<f64>(f, n))) {
                Some((_, Ok(m))) => V::Num(m),
                Some((vm, Err(e))) => {
                    vm.report(e);
                    V::Num(n)
                }
                None => V::Num(n),
            },
        }
    }
}

/// One tween: `from` to `to` between `start` and `end` (ms, clip time).
#[derive(Clone)]
pub(crate) struct Seg {
    pub start: f64,
    pub end: f64,
    pub from: V,
    pub to: V,
    pub ease: Ease,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Composition {
    Replace,
    None,
    Add,
}

/// The tweens of one property of one target, in time order.
#[derive(Clone)]
pub(crate) struct Track {
    pub target: Target,
    pub key: Key,
    pub segs: Vec<Seg>,
    pub composition: Composition,
    pub modifier: Option<Modifier>,
    /// The unit values were written in, kept for state and table targets.
    pub unit: Option<Rc<str>>,
    /// The value before this clip began, for `revert`.
    pub original: Option<V>,
    /// Set when a later `replace` animation took the property over: the
    /// track stops writing without a lookup per frame.
    pub overridden: Rc<Cell<bool>>,
    /// A motion path: the tween's 0..1 progress picks a point or angle on it.
    pub path: Option<(Rc<super::svg::Sampler>, super::svg::Axis)>,
}
impl Track {
    /// The value at clip time `t`: the first tween's start before it, the
    /// last one's end after it.
    pub fn at(&self, t: f64, vm: Option<&Vm>) -> Option<V> {
        let first = self.segs.first()?;
        let i = self.segs.partition_point(|s| s.start <= t);
        let value = if i == 0 {
            first.from.clone()
        } else {
            let seg = &self.segs[i - 1];
            if t >= seg.end || seg.end <= seg.start {
                seg.to.clone()
            } else {
                let p = (t - seg.start) / (seg.end - seg.start);
                seg.from.lerp(&seg.to, seg.ease.at(p, vm))
            }
        };
        let value = match (&self.path, &value) {
            (Some((sampler, axis)), V::Num(progress)) => V::Num(sampler.at(*progress, *axis)),
            _ => value,
        };
        Some(match &self.modifier {
            Some(m) => m.apply(value, vm),
            None => value,
        })
    }
    pub fn end(&self) -> f64 {
        self.segs.iter().map(|s| s.end).fold(0., f64::max)
    }
}

/// Clip-level timing, as anime.js timers have it (all times in ms).
#[derive(Clone)]
pub(crate) struct Timing {
    pub delay: f64,
    /// One iteration's length; infinite for an endless timer.
    pub iteration: f64,
    /// Repeats after the first iteration; infinite loops forever.
    pub loops: f64,
    pub loop_delay: f64,
    pub alternate: bool,
    pub reversed: bool,
    pub speed: f64,
    pub playback_ease: Option<Ease>,
    pub frame_rate: Option<f64>,
    pub autoplay: bool,
}
impl Default for Timing {
    fn default() -> Self {
        Self {
            delay: 0.,
            iteration: 0.,
            loops: 0.,
            loop_delay: 0.,
            alternate: false,
            reversed: false,
            speed: 1.,
            playback_ease: None,
            frame_rate: None,
            autoplay: true,
        }
    }
}
impl Timing {
    pub fn iterations(&self) -> f64 {
        self.loops + 1.
    }
    /// Delay plus every iteration and the delays between them.
    pub fn total(&self) -> f64 {
        if !self.iteration.is_finite() || !self.loops.is_finite() {
            return f64::INFINITY;
        }
        self.delay + self.iterations() * self.iteration + self.loops * self.loop_delay
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Event {
    Begin,
    BeforeUpdate,
    Update,
    Render,
    Loop,
    Pause,
    Complete,
}

#[derive(Default)]
pub(crate) struct Callbacks {
    pub begin: Option<mlua::Function>,
    pub before_update: Option<mlua::Function>,
    pub update: Option<mlua::Function>,
    pub render: Option<mlua::Function>,
    pub loop_: Option<mlua::Function>,
    pub pause: Option<mlua::Function>,
    pub complete: Option<mlua::Function>,
    /// `then(fn)`: run once on completion.
    pub then: Vec<mlua::Function>,
}
impl Callbacks {
    pub fn get(&self, event: Event) -> Option<&mlua::Function> {
        match event {
            Event::Begin => self.begin.as_ref(),
            Event::BeforeUpdate => self.before_update.as_ref(),
            Event::Update => self.update.as_ref(),
            Event::Render => self.render.as_ref(),
            Event::Loop => self.loop_.as_ref(),
            Event::Pause => self.pause.as_ref(),
            Event::Complete => self.complete.as_ref(),
        }
    }
}

/// Something a timeline does at a time: a `call`, or a child's callbacks.
pub(crate) enum Mark {
    Call(mlua::Function),
    Child(Rc<Callbacks>, Event),
}

/// A child of a timeline whose update callbacks run while the timeline is
/// within its span.
pub(crate) struct Span {
    pub start: f64,
    pub end: f64,
    pub callbacks: Rc<Callbacks>,
}

/// Builds a clip's tracks the first time it plays or is asked about: a
/// script creates animations before the elements they target mount.
pub(crate) type Builder = Box<dyn Fn(&Rc<Clip>) -> Result<(), String>>;

#[derive(Default)]
struct State {
    /// Position from 0 to `total`, delay included.
    time: f64,
    paused: bool,
    began: bool,
    completed: bool,
    backwards: bool,
    iteration: f64,
    /// Iteration time last rendered, for timeline marks.
    rendered: Option<f64>,
    /// Time banked toward the next frame when a frame rate is set.
    banked: f64,
    built: bool,
    /// Built before a target was laid out: a percentage of its size read
    /// zero, so the first frame builds again.
    provisional: bool,
    playing: bool,
    /// Has advanced at least once.
    advanced: bool,
}

pub(crate) struct Clip {
    pub id: u64,
    engine: Weak<Engine>,
    pub owner: Weak<Vm>,
    pub timing: RefCell<Timing>,
    state: RefCell<State>,
    pub tracks: RefCell<Vec<Track>>,
    pub marks: RefCell<Vec<(f64, Mark)>>,
    pub spans: RefCell<Vec<Span>>,
    pub labels: RefCell<HashMap<String, f64>>,
    pub callbacks: RefCell<Rc<Callbacks>>,
    builder: RefCell<Option<Builder>>,
    pub label: RefCell<Option<String>>,
    /// A timeline's operations and child defaults, replayed when it builds.
    pub ops: RefCell<Vec<super::build::TlOp>>,
    pub defaults: RefCell<Option<mlua::Table>>,
    pub is_timeline: Cell<bool>,
    /// What lives as long as the clip (a scroll observer's effect).
    pub keep: RefCell<Vec<Box<dyn std::any::Any>>>,
    /// The script line that made it (`anime.animate`), for warnings.
    pub site: RefCell<Option<Rc<Site>>>,
}

/// A script line that makes animations, and how many times in a row its
/// newest one replaced its previous one before that finished.
pub(crate) struct Site {
    at: String,
    warnings: Option<crate::SharedWarnings>,
    restarts: Cell<u32>,
}
/// Replacements in a row before a site's restarts are reported.
const RESTARTS: u32 = 20;

impl Clip {
    pub fn new(
        engine: &Rc<Engine>,
        owner: Weak<Vm>,
        timing: Timing,
        builder: Option<Builder>,
    ) -> Rc<Self> {
        let id = engine.next_id.get() + 1;
        engine.next_id.set(id);
        Rc::new(Self {
            id,
            engine: Rc::downgrade(engine),
            owner,
            timing: RefCell::new(timing),
            state: RefCell::default(),
            tracks: RefCell::default(),
            marks: RefCell::default(),
            spans: RefCell::default(),
            labels: RefCell::default(),
            callbacks: RefCell::default(),
            builder: RefCell::new(builder),
            label: RefCell::default(),
            ops: RefCell::default(),
            defaults: RefCell::default(),
            is_timeline: Cell::new(false),
            keep: RefCell::default(),
            site: RefCell::default(),
        })
    }
    pub fn engine(&self) -> Option<Rc<Engine>> {
        self.engine.upgrade()
    }
    /// Builds the tracks if not yet built. Errors are reported to the
    /// script's error banner. A build before the clip first plays that finds
    /// nothing to animate is not kept: a script asks for durations before the
    /// elements it targets mount.
    pub fn ensure_built(self: &Rc<Self>) {
        if self.state.borrow().built {
            return;
        }
        let playing = self.state.borrow().advanced;
        self.state.borrow_mut().built = true;
        let builder = self.builder.borrow_mut().take();
        super::target::UNMEASURED.with(|u| u.set(false));
        if let Some(builder) = &builder
            && let Err(error) = builder(self)
            && let Some(vm) = self.owner.upgrade()
        {
            vm.report(mlua::Error::runtime(error));
        }
        *self.builder.borrow_mut() = builder;
        self.state.borrow_mut().provisional =
            !playing && super::target::UNMEASURED.with(|u| u.get());
        if !playing && self.tracks.borrow().is_empty() && !self.is_timeline.get() {
            self.state.borrow_mut().built = false;
        }
    }
    /// Forgets the built tracks so the next use builds again (a timeline
    /// that gained children, `refresh`).
    pub fn invalidate(&self) {
        self.state.borrow_mut().built = false;
    }
    pub fn total(self: &Rc<Self>) -> f64 {
        self.ensure_built();
        self.timing.borrow().total()
    }
    pub fn time(&self) -> f64 {
        self.state.borrow().time
    }
    pub fn is_paused(&self) -> bool {
        self.state.borrow().paused
    }
    pub fn is_completed(&self) -> bool {
        self.state.borrow().completed
    }
    pub fn has_began(&self) -> bool {
        self.state.borrow().began
    }
    pub fn is_backwards(&self) -> bool {
        self.state.borrow().backwards
    }
    pub fn iteration(&self) -> f64 {
        self.state.borrow().iteration
    }
    pub fn is_playing(&self) -> bool {
        let s = self.state.borrow();
        s.playing && !s.paused && !s.completed
    }

    fn queue(self: &Rc<Self>, event: Event) {
        if let Some(engine) = self.engine() {
            engine.queue.borrow_mut().push((self.clone(), event));
        }
    }

    /// Advances by `dt` ms of engine time and renders.
    fn advance(self: &Rc<Self>, dt: f64, engine: &Engine) {
        {
            let mut s = self.state.borrow_mut();
            if !s.advanced && s.provisional {
                s.built = false;
                s.provisional = false;
            }
            s.advanced = true;
        }
        self.ensure_built();
        let (speed, frame_rate, total) = {
            let t = self.timing.borrow();
            (t.speed, t.frame_rate, t.total())
        };
        let step = {
            let mut s = self.state.borrow_mut();
            if s.paused || s.completed {
                return;
            }
            let mut step = dt * speed;
            if let Some(fps) = frame_rate.filter(|f| *f > 0.) {
                s.banked += step;
                if s.banked + 0.5 < 1000. / fps {
                    return;
                }
                step = std::mem::take(&mut s.banked);
            }
            if s.backwards { -step } else { step }
        };
        if self.callbacks.borrow().before_update.is_some() {
            self.queue(Event::BeforeUpdate);
        }
        let time = {
            let mut s = self.state.borrow_mut();
            s.time = (s.time + step).clamp(0., total);
            s.time
        };
        self.render(time, engine, false);
        let done = {
            let s = self.state.borrow();
            if s.backwards {
                time <= 0.
            } else {
                time >= total
            }
        };
        if self.callbacks.borrow().update.is_some() {
            self.queue(Event::Update);
        }
        if done {
            self.finish();
        }
    }

    fn finish(self: &Rc<Self>) {
        {
            let mut s = self.state.borrow_mut();
            if s.completed {
                return;
            }
            s.completed = true;
            s.playing = false;
        }
        if let Some(site) = &*self.site.borrow() {
            site.restarts.set(0);
        }
        self.queue(Event::Complete);
    }

    /// Renders the tracks for clip position `time`.
    pub(crate) fn render(self: &Rc<Self>, time: f64, engine: &Engine, seeking: bool) {
        let timing = self.timing.borrow().clone();
        let local = time - timing.delay;
        let backwards = self.state.borrow().backwards;
        if local < 0. && !backwards && !self.state.borrow().began {
            return;
        }
        if !self.state.borrow().began {
            self.state.borrow_mut().began = true;
            self.claim(engine);
            if !seeking {
                self.queue(Event::Begin);
            }
        }
        let local = local.max(0.);
        let iteration_length = timing.iteration;
        let (iteration, mut t) = if !iteration_length.is_finite() {
            (0., local)
        } else {
            let cycle = iteration_length + timing.loop_delay;
            let iterations = timing.iterations();
            let index = if cycle > 0. {
                (local / cycle).floor()
            } else {
                0.
            };
            if index >= iterations || local >= timing.total() - timing.delay {
                (iterations - 1., iteration_length)
            } else {
                (index, (local - index * cycle).min(iteration_length))
            }
        };
        if iteration_length.is_finite() && iteration_length > 0. {
            if timing.alternate && iteration % 2. == 1. {
                t = iteration_length - t;
            }
            if timing.reversed {
                t = iteration_length - t;
            }
            if let Some(ease) = &timing.playback_ease {
                t = ease.at(t / iteration_length, self.owner.upgrade().as_deref())
                    * iteration_length;
            }
        }
        let previous_iteration =
            std::mem::replace(&mut self.state.borrow_mut().iteration, iteration);
        if previous_iteration != iteration && !seeking {
            self.queue(Event::Loop);
        }
        let vm = self.owner.upgrade();
        for track in self.tracks.borrow().iter() {
            if !engine.writes(self.id, track) {
                continue;
            }
            if let Some(value) = track.at(t, vm.as_deref()) {
                engine.write(track, value);
            }
        }
        self.fire_marks(t, previous_iteration != iteration, seeking);
        if self.callbacks.borrow().render.is_some() {
            self.queue(Event::Render);
        }
    }

    fn fire_marks(self: &Rc<Self>, t: f64, wrapped: bool, seeking: bool) {
        let previous = self.state.borrow_mut().rendered.replace(t);
        if seeking {
            return;
        }
        let spans: Vec<Rc<Callbacks>> = self
            .spans
            .borrow()
            .iter()
            .filter(|s| s.start <= t && t <= s.end && s.callbacks.update.is_some())
            .map(|s| s.callbacks.clone())
            .collect();
        let Some(engine) = self.engine() else { return };
        for callbacks in spans {
            engine
                .child_queue
                .borrow_mut()
                .push((callbacks, Event::Update, self.clone()));
        }
        let marks = self.marks.borrow();
        if marks.is_empty() {
            return;
        }
        let (from, to) = match previous {
            Some(p) if !wrapped => (p, t),
            _ => (-1., t),
        };
        let crossed = |at: f64| {
            if to >= from {
                at > from && at <= to
            } else {
                at <= from && at > to
            }
        };
        for (at, mark) in marks.iter().filter(|(at, _)| crossed(*at)) {
            let _ = at;
            match mark {
                Mark::Call(f) => engine.calls.borrow_mut().push((f.clone(), self.clone())),
                Mark::Child(callbacks, event) => {
                    engine
                        .child_queue
                        .borrow_mut()
                        .push((callbacks.clone(), *event, self.clone()))
                }
            }
        }
    }

    /// Takes over this clip's `replace` properties from earlier clips and
    /// notes each property's value before it, for `revert`.
    fn claim(self: &Rc<Self>, engine: &Engine) {
        let mut tracks = self.tracks.borrow_mut();
        for track in tracks.iter_mut() {
            if track.original.is_none() {
                track.original = Some(track.target.read(&track.key));
            }
            if track.composition == Composition::Replace {
                engine.claim(track, self.id);
            }
        }
    }

    /// Writes every track's starting value now, before the first frame: an
    /// animation of mounted elements shows its `from` values at once rather
    /// than its targets' own for a frame.
    pub fn prime(self: &Rc<Self>) {
        self.ensure_built();
        let Some(engine) = self.engine() else { return };
        let vm = self.owner.upgrade();
        engine.runtime.clone().batch(|| {
            for track in self.tracks.borrow().iter() {
                if track.composition == Composition::Add {
                    continue;
                }
                if let Some(value) = track.at(0., vm.as_deref()) {
                    engine.write(track, value);
                }
            }
            engine.flush();
        });
    }

    // Playback controls, as anime.js names them.

    pub fn play(self: &Rc<Self>) {
        {
            let mut s = self.state.borrow_mut();
            if s.completed && !s.backwards {
                s.time = 0.;
                s.began = false;
                s.iteration = 0.;
                s.rendered = None;
            }
            s.completed = false;
            s.paused = false;
            s.backwards = false;
        }
        self.start();
    }
    pub fn resume(self: &Rc<Self>) {
        let completed = self.state.borrow().completed;
        if completed {
            return;
        }
        self.state.borrow_mut().paused = false;
        self.start();
    }
    pub fn pause(self: &Rc<Self>) {
        let was = std::mem::replace(&mut self.state.borrow_mut().paused, true);
        if !was {
            self.queue(Event::Pause);
            if let Some(engine) = self.engine() {
                engine.wake();
            }
        }
    }
    pub fn reverse(self: &Rc<Self>) {
        {
            let mut s = self.state.borrow_mut();
            s.backwards = true;
            s.paused = false;
            if s.completed && s.time > 0. {
                s.completed = false;
            }
        }
        self.start();
    }
    pub fn alternate(self: &Rc<Self>) {
        {
            let mut s = self.state.borrow_mut();
            s.backwards = !s.backwards;
            s.paused = false;
            s.completed = false;
        }
        self.start();
    }
    pub fn restart(self: &Rc<Self>) {
        {
            let mut s = self.state.borrow_mut();
            s.time = 0.;
            s.began = false;
            s.completed = false;
            s.paused = false;
            s.backwards = false;
            s.iteration = 0.;
            s.rendered = None;
        }
        self.start();
    }
    /// Back to the start, rendered there, paused.
    pub fn reset(self: &Rc<Self>) {
        {
            let mut s = self.state.borrow_mut();
            s.time = 0.;
            s.completed = false;
            s.paused = true;
            s.backwards = false;
            s.iteration = 0.;
            s.rendered = None;
        }
        self.seek(0.);
    }
    pub fn seek(self: &Rc<Self>, time: f64) {
        let total = self.total();
        let time = time.clamp(0., total.min(f64::MAX));
        self.state.borrow_mut().time = time;
        if let Some(engine) = self.engine() {
            engine.runtime.clone().batch(|| {
                self.render(time, &engine, true);
                engine.flush();
            });
        }
    }
    pub fn complete(self: &Rc<Self>) {
        let total = self.total();
        if total.is_finite() {
            self.state.borrow_mut().time = total;
            if let Some(engine) = self.engine() {
                engine.runtime.clone().batch(|| {
                    self.render(total, &engine, false);
                    engine.flush();
                });
            }
        }
        self.finish();
        if let Some(engine) = self.engine() {
            engine.drain();
        }
    }
    /// Stops without completing; values stay where they are.
    /// Whether the clip animates only elements, all removed since mounting.
    /// An endless loop whose elements all sit where nothing shows: in a
    /// closed dialog, under a faded-out ancestor, clipped away. Their own
    /// opacity doesn't count (a blink fades its element out and back).
    /// Advancing it would draw nothing, so it waits; a finite animation
    /// always runs on, to finish and call back.
    fn unseen_loop(&self) -> bool {
        if !self.timing.borrow().loops.is_infinite() {
            return false;
        }
        let tracks = self.tracks.borrow();
        !tracks.is_empty()
            && tracks.iter().all(|t| match &t.target {
                Target::Element(e) => e
                    .read(|scene, id| {
                        scene
                            .parent(id)
                            .is_some_and(|parent| scene.visible_bounds(parent).is_none())
                    })
                    .unwrap_or(false),
                _ => false,
            })
    }
    fn targets_gone(&self) -> bool {
        let tracks = self.tracks.borrow();
        !tracks.is_empty()
            && tracks
                .iter()
                .all(|t| matches!(&t.target, Target::Element(e) if e.is_gone()))
    }
    pub fn cancel(self: &Rc<Self>) {
        {
            let mut s = self.state.borrow_mut();
            s.paused = true;
            s.playing = false;
        }
        if let Some(engine) = self.engine() {
            for track in self.tracks.borrow().iter() {
                engine.release(&track.target, &track.key, self.id);
            }
        }
    }
    /// Cancels and restores every property to its value before the clip.
    pub fn revert(self: &Rc<Self>) {
        self.cancel();
        let Some(engine) = self.engine() else { return };
        engine.runtime.clone().batch(|| {
            for track in self.tracks.borrow().iter().rev() {
                if let Some(original) = &track.original {
                    engine.write(track, original.clone());
                }
            }
            engine.flush();
        });
        let mut s = self.state.borrow_mut();
        s.time = 0.;
        s.began = false;
        s.completed = false;
    }
    /// Scales every time so the whole clip lasts `duration` ms.
    pub fn stretch(self: &Rc<Self>, duration: f64) {
        self.ensure_built();
        let old = self.timing.borrow().iteration;
        if !(old > 0. && old.is_finite() && duration > 0.) {
            return;
        }
        let iterations = self.timing.borrow().iterations();
        let factor = duration / iterations / old;
        self.timing.borrow_mut().iteration = old * factor;
        for track in self.tracks.borrow_mut().iter_mut() {
            for seg in &mut track.segs {
                seg.start *= factor;
                seg.end *= factor;
            }
        }
        for (at, _) in self.marks.borrow_mut().iter_mut() {
            *at *= factor;
        }
    }
    pub fn set_speed(&self, speed: f64) {
        self.timing.borrow_mut().speed = speed.max(0.);
    }

    /// Hands the clip to the engine's playing list.
    pub fn start(self: &Rc<Self>) {
        self.state.borrow_mut().playing = true;
        if let Some(engine) = self.engine() {
            engine.play(self);
        }
    }
}

/// A child's callback a timeline fires, with the timeline.
type ChildEvent = (Rc<Callbacks>, Event, Rc<Clip>);
/// Per-frame Rust work, given the frame's ms; false when finished.
pub(crate) type Ticker = Rc<dyn Fn(f64) -> bool>;

/// The clip that owns a property, and its track's override flag.
pub(crate) struct Owner {
    pub clip: u64,
    pub track: Rc<Cell<bool>>,
}

struct AddSlot {
    target: Target,
    key: Key,
    unit: Option<Rc<str>>,
    base: f64,
    sum: f64,
    touched: bool,
}

/// Wakes the engine's task when something starts playing.
#[derive(Default)]
struct Notify {
    pending: Cell<bool>,
    waker: RefCell<Option<Waker>>,
}
impl Notify {
    fn notify(&self) {
        self.pending.set(true);
        if let Some(waker) = self.waker.borrow_mut().take() {
            waker.wake();
        }
    }
    async fn wait(&self) {
        poll_fn(|cx| {
            if self.pending.replace(false) {
                Poll::Ready(())
            } else {
                *self.waker.borrow_mut() = Some(cx.waker().clone());
                Poll::Pending
            }
        })
        .await
    }
}

/// How often an engine whose loops all sit unseen looks again (Hz).
const UNSEEN_RATE: f64 = 4.;

pub(crate) struct Engine {
    pub runtime: Runtime,
    pub elements: Rc<crate::element::Elements>,
    clips: RefCell<Vec<Rc<Clip>>>,
    /// Reused each tick so playing does not allocate per frame.
    scratch: RefCell<Vec<Rc<Clip>>>,
    /// Every playing clip last frame was an endless loop no one could see
    /// (in a closed dialog, a faded-out or clipped-away subtree): the engine
    /// then only looks again a few times a second (`UNSEEN_RATE`).
    unseen: Cell<bool>,
    wake: Notify,
    dirty: RefCell<Vec<Rc<Element>>>,
    queue: RefCell<Vec<(Rc<Clip>, Event)>>,
    child_queue: RefCell<Vec<ChildEvent>>,
    calls: RefCell<Vec<(mlua::Function, Rc<Clip>)>>,
    /// Owners of state and table properties; elements keep their own.
    owners: RefCell<HashMap<(usize, Key), Owner>>,
    sites: RefCell<HashMap<String, Rc<Site>>>,
    adds: RefCell<HashMap<(usize, Key), AddSlot>>,
    pub(crate) next_id: Cell<u64>,
    /// `engine.speed`, `engine.fps`, `engine.timeUnit` (seconds when true).
    pub speed: Cell<f64>,
    pub fps: Cell<Option<f64>>,
    pub paused: Cell<bool>,
    pub seconds: Cell<bool>,
    /// Engine time in ms, advanced per frame.
    pub now: Cell<f64>,
    /// Frames the engine has run, for tests and diagnostics.
    pub frames: Cell<u64>,
    /// Rust per-frame work (drag inertia, smoothed scroll sync): each runs
    /// with the frame's ms until it returns false.
    tickers: RefCell<Vec<Ticker>>,
    /// Work waiting for elements to mount (draggables and scroll observers
    /// created before their targets): retried once after each mount pass.
    pending: RefCell<Vec<Rc<dyn Fn() -> bool>>>,
    /// Clips kept without playing (scroll-linked), until their script goes.
    held: RefCell<Vec<Rc<Clip>>>,
    /// Scopes being added to (`createScope():add(fn)`), innermost last.
    pub(crate) scopes: RefCell<Vec<(Option<mlua::Table>, Weak<super::api::Scope>)>>,
}

impl Engine {
    pub fn new(runtime: &Runtime, elements: Rc<crate::element::Elements>) -> Rc<Self> {
        Rc::new(Self {
            runtime: runtime.clone(),
            elements,
            clips: RefCell::default(),
            scratch: RefCell::default(),
            unseen: Cell::new(false),
            wake: Notify::default(),
            dirty: RefCell::default(),
            queue: RefCell::default(),
            child_queue: RefCell::default(),
            calls: RefCell::default(),
            owners: RefCell::default(),
            sites: RefCell::default(),
            adds: RefCell::default(),
            next_id: Cell::new(0),
            speed: Cell::new(1.),
            fps: Cell::new(None),
            paused: Cell::new(false),
            seconds: Cell::new(false),
            now: Cell::new(0.),
            frames: Cell::new(0),
            tickers: RefCell::default(),
            pending: RefCell::default(),
            held: RefCell::default(),
            scopes: RefCell::default(),
        })
    }
    /// Script time to ms (`engine.timeUnit`).
    pub fn ms(&self, value: f64) -> f64 {
        if self.seconds.get() {
            value * 1000.
        } else {
            value
        }
    }
    pub fn script_time(&self, value: f64) -> f64 {
        if self.seconds.get() {
            value / 1000.
        } else {
            value
        }
    }
    fn play(&self, clip: &Rc<Clip>) {
        let mut clips = self.clips.borrow_mut();
        if !clips.iter().any(|c| Rc::ptr_eq(c, clip)) {
            clips.push(clip.clone());
        }
        drop(clips);
        self.wake.notify();
    }
    pub fn wake(&self) {
        self.wake.notify();
    }
    /// The frame rate the playing work needs: when every playing clip caps
    /// its own (`frameRate`), the highest of those caps, so a 12 fps loop
    /// wakes the app 12 times a second rather than at the display's rate.
    /// `None`: something wants every frame.
    fn needed_rate(&self) -> Option<f64> {
        if !self.tickers.borrow().is_empty() {
            return None;
        }
        if self.unseen.get() {
            return Some(UNSEEN_RATE);
        }
        let mut rate: Option<f64> = None;
        for clip in self.clips.borrow().iter().filter(|c| c.is_playing()) {
            let cap = clip.timing.borrow().frame_rate.filter(|f| *f > 0.)?;
            // Slowing a clip down only makes its values change less often,
            // so its cap stays an upper bound.
            rate = Some(rate.map_or(cap, |r: f64| r.max(cap)));
        }
        rate
    }
    pub fn is_active(&self) -> bool {
        !self.paused.get()
            && (!self.tickers.borrow().is_empty()
                || self.clips.borrow().iter().any(|c| c.is_playing()))
    }
    /// Keeps `clip` while its script lives.
    pub(crate) fn hold(&self, clip: Rc<Clip>) {
        self.held.borrow_mut().push(clip);
    }
    /// Runs `ticker` every frame until it returns false.
    pub(crate) fn tick_with(&self, ticker: Ticker) {
        self.tickers.borrow_mut().push(ticker);
        self.wake.notify();
    }
    /// Retries `attach` after mounts until it returns true.
    pub(crate) fn defer(&self, attach: Rc<dyn Fn() -> bool>) {
        if !attach() {
            self.pending.borrow_mut().push(attach);
            self.elements.watch(true);
        }
    }
    /// Retries deferred work; called once after elements mount.
    fn attach_pending(&self) {
        let pending = std::mem::take(&mut *self.pending.borrow_mut());
        let left: Vec<_> = pending.into_iter().filter(|attach| !attach()).collect();
        self.elements.watch(!left.is_empty());
        self.pending.borrow_mut().extend(left);
    }
    /// A one-property animation built in Rust (a draggable's release).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn tween(
        self: &Rc<Self>,
        owner: Weak<Vm>,
        writes: Vec<(Target, Key, V)>,
        duration: f64,
        ease: Ease,
        callbacks: Callbacks,
    ) -> Rc<Clip> {
        let tracks = writes
            .into_iter()
            .map(|(target, key, to)| Track {
                segs: vec![Seg {
                    start: 0.,
                    end: duration,
                    from: target.read(&key),
                    to,
                    ease: ease.clone(),
                }],
                target,
                key,
                composition: Composition::Replace,
                modifier: None,
                unit: None,
                original: None,
                overridden: Rc::default(),
                path: None,
            })
            .collect();
        let clip = Clip::new(
            self,
            owner,
            Timing {
                iteration: duration,
                ..Default::default()
            },
            None,
        );
        *clip.tracks.borrow_mut() = tracks;
        *clip.callbacks.borrow_mut() = Rc::new(callbacks);
        clip.start();
        clip
    }
    /// Cancels clips whose script was replaced.
    pub fn drop_orphans(&self) {
        let orphans: Vec<Rc<Clip>> = self
            .clips
            .borrow()
            .iter()
            .filter(|c| c.owner.strong_count() == 0)
            .cloned()
            .collect();
        for clip in orphans {
            clip.cancel();
        }
        self.clips
            .borrow_mut()
            .retain(|c| c.owner.strong_count() > 0);
        self.held
            .borrow_mut()
            .retain(|c| c.owner.strong_count() > 0);
    }
    /// Ticks once per display frame while anything plays.
    pub async fn run(self: Rc<Self>, frames: FrameClock) {
        loop {
            while !self.is_active() {
                self.wake.wait().await;
                if self.elements.mounted_since_watch() {
                    self.attach_pending();
                }
            }
            let mut last: Option<Instant> = None;
            while self.is_active() {
                let rate = match (self.fps.get(), self.needed_rate()) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    (a, b) => a.or(b),
                };
                let frame = match rate {
                    Some(hz) => frames.next().max_rate(hz).await,
                    None => frames.next().await,
                };
                let dt = last.map_or(0., |l| (frame.time - l).as_secs_f64() * 1000.);
                last = Some(frame.time);
                self.tick(dt);
            }
        }
    }

    /// Advances every playing clip by `dt` ms and writes the results.
    pub fn tick(&self, dt: f64) {
        if self.paused.get() {
            return;
        }
        self.frames.set(self.frames.get() + 1);
        self.now.set(self.now.get() + dt);
        let mut playing = std::mem::take(&mut *self.scratch.borrow_mut());
        playing.clear();
        playing.extend(
            self.clips
                .borrow()
                .iter()
                .filter(|c| c.is_playing() && c.owner.strong_count() > 0)
                .cloned(),
        );
        let dt = dt * self.speed.get();
        if self.frames.get().is_multiple_of(32) {
            // Animations whose elements have all left the interface end, so
            // a loop on a removed row does not keep frames coming.
            playing.retain(|clip| {
                let gone = clip.targets_gone();
                if gone {
                    clip.cancel();
                }
                !gone
            });
        }
        if self.elements.mounted_since_watch() {
            self.attach_pending();
        }
        // Loops no one can see wait (their time stands still) until they show.
        playing.retain(|clip| !clip.unseen_loop());
        let tickers = self.tickers.borrow().clone();
        self.unseen.set(playing.is_empty() && tickers.is_empty());
        self.runtime.clone().batch(|| {
            for clip in &playing {
                clip.advance(dt, self);
            }
            if !tickers.is_empty() {
                let done: Vec<usize> = tickers
                    .iter()
                    .enumerate()
                    .filter(|(_, t)| !t(dt))
                    .map(|(i, _)| i)
                    .collect();
                let mut live = self.tickers.borrow_mut();
                live.retain(|t| !done.iter().any(|&i| Rc::ptr_eq(&tickers[i], t)));
            }
            self.flush();
        });
        playing.clear();
        *self.scratch.borrow_mut() = playing;
        self.clips
            .borrow_mut()
            .retain(|c| c.is_playing() && c.owner.strong_count() > 0);
        self.drain();
    }

    /// Whether `track` may write: `replace` tracks write until a later
    /// clip takes their property.
    fn writes(&self, _clip: u64, track: &Track) -> bool {
        track.composition != Composition::Replace || !track.overridden.get()
    }
    /// Makes `track` (of `clip`) its property's owner; the previous owner's
    /// track stops writing.
    fn claim(&self, track: &Track, clip: u64) {
        let owner = Owner {
            clip,
            track: track.overridden.clone(),
        };
        track.overridden.set(false);
        let previous = match &track.target {
            Target::Element(e) => {
                let mut owners = e.owners.borrow_mut();
                match owners.iter_mut().find(|(k, _)| *k == track.key) {
                    Some((_, current)) => Some(std::mem::replace(current, owner)),
                    None => {
                        owners.push((track.key.clone(), owner));
                        None
                    }
                }
            }
            other => self
                .owners
                .borrow_mut()
                .insert((other.addr(), track.key.clone()), owner),
        };
        if let Some(previous) = previous
            && previous.clip != clip
        {
            previous.track.set(true);
            self.overridden(previous.clip, clip);
        }
    }
    /// The site (script line) `at`, shared by every clip it makes.
    pub(crate) fn site(&self, at: String, warnings: Option<crate::SharedWarnings>) -> Rc<Site> {
        self.sites
            .borrow_mut()
            .entry(at.clone())
            .or_insert_with(|| {
                Rc::new(Site {
                    at,
                    warnings,
                    restarts: Cell::new(0),
                })
            })
            .clone()
    }
    /// A later clip (`by`) took a property of `previous`. Once every one of
    /// its tracks is taken it has nothing left to write: as in anime.js, it
    /// is cancelled (a timeline pauses), so it neither keeps frames coming
    /// (a replaced loop otherwise ran on unseen) nor calls back. A line
    /// that keeps replacing its own unfinished animation is reported: what
    /// drives it changes faster than it plays, so the value only chases.
    fn overridden(&self, previous: u64, by: u64) {
        let find = |id| self.clips.borrow().iter().find(|c| c.id == id).cloned();
        let Some(previous) = find(previous) else {
            return;
        };
        let spent = {
            let tracks = previous.tracks.borrow();
            !tracks.is_empty()
                && tracks
                    .iter()
                    .all(|t| t.composition == Composition::Replace && t.overridden.get())
        };
        if !spent || !previous.is_playing() {
            return;
        }
        if previous.is_timeline.get() {
            previous.pause();
        } else {
            previous.cancel();
        }
        let site = previous.site.borrow().clone();
        if let Some(site) = site
            && find(by).is_some_and(|by| {
                by.site
                    .borrow()
                    .as_ref()
                    .is_some_and(|s| Rc::ptr_eq(s, &site))
            })
        {
            site.restarts.set(site.restarts.get() + 1);
            if site.restarts.get() == RESTARTS {
                crate::warning(
                    site.warnings.as_ref(),
                    format!(
                        "{}: this `anime.animate` replaced its own animation before it \
                         finished {RESTARTS} times in a row: what drives it changes faster \
                         than the animation plays, so the value only chases its target. \
                         Keep one `anime.createAnimatable` and set its value, or bind the \
                         property to `ui.tween(state)` or `ui.spring(state)`, which retarget \
                         in Rust.",
                        site.at
                    ),
                );
            }
        }
    }
    fn release(&self, target: &Target, key: &Key, clip: u64) {
        match target {
            Target::Element(e) => e
                .owners
                .borrow_mut()
                .retain(|(k, o)| !(k == key && o.clip == clip)),
            other => {
                let mut owners = self.owners.borrow_mut();
                if owners
                    .get(&(other.addr(), key.clone()))
                    .is_some_and(|o| o.clip == clip)
                {
                    owners.remove(&(other.addr(), key.clone()));
                }
            }
        }
    }

    /// Writes one value: additive values collect per property and frame.
    pub(crate) fn write(&self, track: &Track, value: V) {
        if track.composition != Composition::Add && self.adds.borrow().is_empty() {
            self.put(&track.target, &track.key, &value, track.unit.as_deref());
            return;
        }
        let slot_key = (track.target.addr(), track.key.clone());
        if track.composition == Composition::Add
            && let Some(n) = value.num()
        {
            let mut adds = self.adds.borrow_mut();
            let slot = adds.entry(slot_key).or_insert_with(|| AddSlot {
                target: track.target.clone(),
                key: track.key.clone(),
                unit: track.unit.clone(),
                base: track
                    .original
                    .as_ref()
                    .and_then(V::num)
                    .unwrap_or_else(|| track.target.read(&track.key).num().unwrap_or(0.)),
                sum: 0.,
                touched: false,
            });
            slot.sum += n;
            slot.touched = true;
            return;
        }
        // Additive tracks set a base for this property when any exist.
        if !self.adds.borrow().is_empty()
            && let Some(slot) = self.adds.borrow_mut().get_mut(&slot_key)
            && let Some(n) = value.num()
        {
            slot.base = n;
            slot.touched = true;
            return;
        }
        self.put(&track.target, &track.key, &value, track.unit.as_deref());
    }
    fn put(&self, target: &Target, key: &Key, value: &V, unit: Option<&str>) {
        if target.write(key, value, unit)
            && let Target::Element(element) = target
            && element.queue()
        {
            self.dirty.borrow_mut().push(element.clone());
        }
    }

    /// Writes this frame's additive sums and every queued element.
    pub(crate) fn flush(&self) {
        {
            let mut adds = self.adds.borrow_mut();
            let touched: Vec<(Target, Key, Option<Rc<str>>, f64)> = adds
                .values_mut()
                .filter_map(|s| {
                    std::mem::take(&mut s.touched).then(|| {
                        let total = s.base + std::mem::take(&mut s.sum);
                        (s.target.clone(), s.key.clone(), s.unit.clone(), total)
                    })
                })
                .collect();
            drop(adds);
            for (target, key, unit, total) in touched {
                self.put(&target, &key, &V::Num(total), unit.as_deref());
            }
        }
        let mut dirty = std::mem::take(&mut *self.dirty.borrow_mut());
        if dirty.is_empty() {
            return;
        }
        let mut restyle = Vec::new();
        // Elements of one window share a scene: upgrade and borrow it once
        // for each run of them.
        let mut index = 0;
        while index < dirty.len() {
            let Some(weak) = dirty[index].scene_weak() else {
                dirty[index].flush_detached();
                index += 1;
                continue;
            };
            let Some(scene) = weak.upgrade() else {
                dirty[index].flush_detached();
                index += 1;
                continue;
            };
            let mut borrowed = scene.borrow_mut();
            while index < dirty.len() && dirty[index].scene_weak().is_some_and(|w| w.ptr_eq(weak)) {
                if dirty[index].flush(&mut borrowed) {
                    restyle.push(dirty[index].clone());
                }
                index += 1;
            }
        }
        // The list keeps its capacity for the next frame's elements.
        dirty.clear();
        let mut queue = self.dirty.borrow_mut();
        if queue.is_empty() {
            *queue = dirty;
        }
        drop(queue);
        for element in restyle {
            element.revision.update(|r| *r = r.wrapping_add(1));
        }
    }

    /// Runs the callbacks this tick queued, outside every borrow: they may
    /// start, stop or seek animations.
    pub(crate) fn drain(&self) {
        loop {
            let queued = std::mem::take(&mut *self.queue.borrow_mut());
            let children = std::mem::take(&mut *self.child_queue.borrow_mut());
            let calls = std::mem::take(&mut *self.calls.borrow_mut());
            if queued.is_empty() && children.is_empty() && calls.is_empty() {
                return;
            }
            for (clip, event) in queued {
                let callbacks = clip.callbacks.borrow().clone();
                let Some(vm) = clip.owner.upgrade() else {
                    continue;
                };
                let handle = super::api::ClipUd(clip.clone());
                if let Some(f) = callbacks.get(event) {
                    let _ = vm.call::<()>(f, handle.clone()).map_err(|e| vm.report(e));
                }
                if event == Event::Complete {
                    for f in &callbacks.then {
                        let _ = vm.call::<()>(f, handle.clone()).map_err(|e| vm.report(e));
                    }
                }
            }
            for (callbacks, event, clip) in children {
                let Some(vm) = clip.owner.upgrade() else {
                    continue;
                };
                if let Some(f) = callbacks.get(event) {
                    let _ = vm
                        .call::<()>(f, super::api::ClipUd(clip.clone()))
                        .map_err(|e| vm.report(e));
                }
            }
            for (f, clip) in calls {
                let Some(vm) = clip.owner.upgrade() else {
                    continue;
                };
                let _ = vm
                    .call::<()>(&f, super::api::ClipUd(clip.clone()))
                    .map_err(|e| vm.report(e));
            }
        }
    }
    /// Applies values now, outside any frame (`utils.set`, `seek`).
    pub(crate) fn set_now(&self, writes: Vec<(Target, Key, V, Option<Rc<str>>)>) {
        self.runtime.clone().batch(|| {
            for (target, key, value, unit) in &writes {
                self.put(target, key, value, unit.as_deref());
            }
            self.flush();
        });
    }
    pub(crate) fn playing_clips(&self) -> Vec<Rc<Clip>> {
        self.clips.borrow().clone()
    }
}
