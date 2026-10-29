//! Single-threaded fine-grained reactivity. Effects subscribe only to values read.
use std::{
    any::{Any, TypeId},
    cell::{Cell, RefCell},
    cmp::Reverse,
    collections::{BinaryHeap, HashMap, HashSet},
    rc::{Rc, Weak},
};

type EffectId = u64;
trait Dependency {
    fn unsubscribe(&self, id: EffectId);
}
/// One allocation: the closure lives in the `Rc` itself.
type Callback = Rc<RefCell<dyn FnMut()>>;
struct Subscription {
    source: Weak<dyn Dependency>,
    /// Identity of the signal, for finding an existing subscription on re-read.
    key: *const (),
    /// The run that last read this dependency.
    run: u64,
}
struct EffectRecord {
    callback: Callback,
    /// Signals this effect subscribes to. A re-run marks each one it reads
    /// again; afterwards only the ones it stopped reading are unsubscribed,
    /// so a steady effect leaves every subscriber set untouched.
    /// Inline for the few most effects read: no allocation of its own.
    dependencies: smallvec::SmallVec<[Subscription; 2]>,
    run: u64,
    /// In `queued` or `pending`, waiting to run: once however many writes.
    queued: bool,
    /// Never to run again (`Effect::stop`); freed when its handle drops.
    stopped: bool,
    /// Reads no layout: a mount runs it without drawing its text inputs
    /// first (`Runtime::batch_watched`).
    mount_pure: bool,
}
/// Effect ids are unique integers; a multiplicative hash beats SipHash here.
#[derive(Default, Clone, Copy)]
struct IdHasher(u64);
impl std::hash::Hasher for IdHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 = (self.0 ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3);
        }
    }
    fn write_u64(&mut self, value: u64) {
        self.0 = value.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    }
}
/// Low bits of an effect's id: its slot in `Effects`.
const SLOT_BITS: u32 = 24;
const SLOT_MASK: EffectId = (1 << SLOT_BITS) - 1;
/// Effects by id: slots, reused once freed. An id is `born << SLOT_BITS |
/// slot`, `born` counting effects made, never reused: ids order effects
/// by age as a plain count does, and a slot remembers whose it is, so a
/// stale id finds nothing.
#[derive(Default)]
struct Effects {
    slots: Vec<(EffectId, Option<EffectRecord>)>,
    free: Vec<u32>,
    live: usize,
}
impl Effects {
    /// Keep `record` as the effect `born`-th made: its id.
    fn insert(&mut self, born: u64, record: EffectRecord) -> EffectId {
        assert!(born < 1 << (64 - SLOT_BITS), "effect identifiers exhausted");
        let slot = match self.free.pop() {
            Some(slot) => slot as usize,
            None => {
                assert!(self.slots.len() < 1 << SLOT_BITS, "too many effects");
                self.slots.push((EffectId::MAX, None));
                self.slots.len() - 1
            }
        };
        let id = born << SLOT_BITS | slot as EffectId;
        self.slots[slot] = (id, Some(record));
        self.live += 1;
        id
    }
    #[inline(always)]
    fn get(&self, id: &EffectId) -> Option<&EffectRecord> {
        match self.slots.get((id & SLOT_MASK) as usize) {
            Some((at, record)) if at == id => record.as_ref(),
            _ => None,
        }
    }
    #[inline(always)]
    fn get_mut(&mut self, id: &EffectId) -> Option<&mut EffectRecord> {
        match self.slots.get_mut((id & SLOT_MASK) as usize) {
            Some((at, record)) if at == id => record.as_mut(),
            _ => None,
        }
    }
    fn remove(&mut self, id: &EffectId) -> Option<EffectRecord> {
        let slot = (id & SLOT_MASK) as usize;
        match self.slots.get_mut(slot) {
            Some((at, record)) if at == id => {
                *at = EffectId::MAX;
                let record = record.take();
                self.live -= 1;
                self.free.push(slot as u32);
                record
            }
            _ => None,
        }
    }
    fn len(&self) -> usize {
        self.live
    }
}
/// A signal's subscribers, in no order: a flush orders what runs. A set
/// most nodes' effects join (the inherited typography) is removed from
/// one by one as they go, in constant time.
type IdSet = HashSet<EffectId, std::hash::BuildHasherDefault<IdHasher>>;
#[derive(Default)]
struct Inner {
    effects: RefCell<Effects>,
    /// Effects to run, the oldest first (they were made before what they
    /// made): those queued outside a flush (a batch's writes, most of them)
    /// in `queued`, sorted once as a flush starts; those queued while it
    /// runs in `pending`, a heap. A flush runs the oldest of either next.
    /// Each keeps its room. A disposed effect's id may stay until it is
    /// taken.
    queued: RefCell<Vec<EffectId>>,
    pending: RefCell<BinaryHeap<Reverse<EffectId>>>,
    /// Room `newest_first` orders `queued` in, kept between flushes.
    order: RefCell<Order>,
    active: Cell<Option<EffectId>>,
    /// Effects made so far (`Effects::insert`'s `born`).
    next: Cell<u64>,
    /// Effects stopped whose handles still live (`Effect::stop`).
    stopped: Cell<usize>,
    batch_depth: Cell<usize>,
    flushing: Cell<bool>,
    /// One value per type that belongs to this runtime (`Runtime::extension`).
    extensions: RefCell<Vec<(TypeId, Rc<dyn Any>)>>,
}

/// The effect running for as long as it lives: whatever ran before runs
/// again when it drops, a panicking callback's unwind included, so later
/// reads never subscribe an effect that is no longer running.
struct Active<'a> {
    slot: &'a Cell<Option<EffectId>>,
    previous: Option<EffectId>,
}
impl<'a> Active<'a> {
    fn run(slot: &'a Cell<Option<EffectId>>, id: EffectId) -> Self {
        Self {
            slot,
            previous: slot.replace(Some(id)),
        }
    }
}
impl Drop for Active<'_> {
    fn drop(&mut self) {
        self.slot.set(self.previous);
    }
}
/// `newest_first`'s room: a bit for each effect born in the range queued,
/// and its slot.
#[derive(Default)]
struct Order {
    bits: Vec<u64>,
    slots: Vec<u32>,
}
/// Sort `ids` newest (highest) first. Subscribers come in no order, but the
/// ids a write queues are mostly close together (effects made together): a
/// bit for each id's `born` in their range, read from the top, orders them
/// in one pass rather than a sort's many (a dashboard queues a thousand a
/// tick). No id is queued twice (`EffectRecord::queued`), so none is lost.
#[inline(always)]
fn newest_first<const IN_PLACE: bool>(ids: &mut Vec<EffectId>, order: &mut Order) {
    // Effects queued as they are made come oldest first: reversed.
    let (mut low, mut high, mut ascending) = (u64::MAX, 0, true);
    let mut last = 0;
    for &id in ids.iter() {
        ascending &= id >= last;
        last = id;
        let bound = if IN_PLACE { id } else { id >> SLOT_BITS };
        low = low.min(bound);
        high = high.max(bound);
    }
    if ascending {
        ids.reverse();
        return;
    }
    let (low, high) = if IN_PLACE {
        (low >> SLOT_BITS, high >> SLOT_BITS)
    } else {
        (low, high)
    };
    let span = high - low;
    if ids.len() < 32 || span / 32 > ids.len() as u64 {
        ids.sort_unstable_by(|a, b| b.cmp(a));
        return;
    }
    let Order { bits, slots } = order;
    bits.clear();
    bits.resize(span as usize / 64 + 1, 0);
    if slots.len() <= span as usize {
        slots.resize(span as usize + 1, 0);
    }
    for id in ids.iter() {
        let at = (id >> SLOT_BITS) - low;
        bits[at as usize / 64] |= 1 << (at % 64);
        slots[at as usize] = (id & SLOT_MASK) as u32;
    }
    let id = |at: u64| (low + at) << SLOT_BITS | EffectId::from(slots[at as usize]);
    // In place, over the ids taken (as many come out): for `flush`. The
    // barrier's flush keeps clearing and pushing, which codegens smaller
    // there (research A's A22).
    if IN_PLACE {
        let mut written = 0;
        for (word, &set) in bits.iter().enumerate().rev() {
            let mut set = set;
            while set != 0 {
                let bit = 63 - set.leading_zeros() as u64;
                ids[written] = id(word as u64 * 64 + bit);
                written += 1;
                set &= !(1 << bit);
            }
        }
        ids.truncate(written);
    } else {
        ids.clear();
        for (word, &set) in bits.iter().enumerate().rev() {
            let mut set = set;
            while set != 0 {
                let bit = 63 - set.leading_zeros() as u64;
                ids.push(id(word as u64 * 64 + bit));
                set &= !(1 << bit);
            }
        }
    }
}
impl Inner {
    /// Queue effect `id` to run: in a flush's heap if one is running.
    fn queue(&self, id: EffectId) {
        match self.flushing.get() {
            true => self.pending.borrow_mut().push(Reverse(id)),
            false => self.queued.borrow_mut().push(id),
        }
    }
    /// Run effect `id` if it is queued. Its heap entry may outlive the
    /// queueing (`Refresh::refresh` ran it early), so an effect that isn't
    /// queued is left alone. Returns whether it ran.
    #[inline(always)]
    fn run(&self, id: EffectId) -> bool {
        let callback = {
            let mut effects = self.effects.borrow_mut();
            let Some(record) = effects.get_mut(&id).filter(|record| record.queued) else {
                return false;
            };
            record.queued = false;
            record.run = record.run.wrapping_add(1);
            record.callback.clone()
        };
        #[cfg(feature = "trace")]
        let traced = crate::trace::enabled().then(std::time::Instant::now);
        {
            let _active = Active::run(&self.active, id);
            (*callback.borrow_mut())();
        }
        #[cfg(feature = "trace")]
        if let Some(started) = traced {
            crate::trace::ran(id, started);
        }
        // Drop subscriptions this run no longer read. The record may be
        // gone if the callback disposed its own effect.
        let stale: Vec<Weak<dyn Dependency>> = {
            let mut effects = self.effects.borrow_mut();
            let Some(record) = effects.get_mut(&id) else {
                return true;
            };
            let run = record.run;
            // `KEPT` is above every run: read this run, or kept.
            if record.dependencies.iter().all(|d| d.run >= run) {
                return true;
            }
            let mut stale = Vec::new();
            record.dependencies.retain(|d| {
                let keep = d.run >= run;
                if !keep {
                    stale.push(d.source.clone());
                }
                keep
            });
            stale
        };
        for dependency in stale {
            if let Some(dependency) = dependency.upgrade() {
                dependency.unsubscribe(id);
            }
        }
        true
    }
}

/// An explicit runtime avoids global state and supports multiple independent UI roots.
#[derive(Clone, Default)]
pub struct Runtime {
    inner: Rc<Inner>,
}
impl Runtime {
    /// A runtime with no signals or effects.
    pub fn new() -> Self {
        Self::default()
    }
    /// Reads inside `callback` do not subscribe the currently running observer.
    ///
    /// This is useful for constructing a retained component inside a reactive
    /// branch: its initial reads belong to the component, not to the branch's
    /// selector. Nested calls and unwinding restore the previous observer.
    /// Effects created inside this callback still track their own dependencies
    /// when they run. Only this runtime's tracking is affected.
    pub fn untracked<R>(&self, callback: impl FnOnce() -> R) -> R {
        struct Guard<'a> {
            active: &'a Cell<Option<EffectId>>,
            previous: Option<EffectId>,
        }
        impl Drop for Guard<'_> {
            fn drop(&mut self) {
                self.active.set(self.previous);
            }
        }
        let _guard = Guard {
            active: &self.inner.active,
            previous: self.inner.active.replace(None),
        };
        callback()
    }
    /// A signal in this runtime holding `value`.
    pub fn signal<T: 'static>(&self, value: T) -> Signal<T> {
        Signal {
            inner: Rc::new(SignalInner {
                value: RefCell::new(value),
                version: Cell::new(0),
                subscribers: RefCell::default(),
                runtime: Rc::downgrade(&self.inner),
            }),
        }
    }
    /// Runs immediately. Keep the returned handle alive to keep the subscription.
    pub fn effect(&self, callback: impl FnMut() + 'static) -> Effect {
        self.effect_marked(false, callback)
    }
    /// [`Runtime::effect`], `mount_pure` if it reads no layout.
    pub(crate) fn effect_marked(
        &self,
        mount_pure: bool,
        callback: impl FnMut() + 'static,
    ) -> Effect {
        self.effect_callback(mount_pure, Rc::new(RefCell::new(callback)))
    }
    /// [`Runtime::effect_marked`] with its callback already allocated: a
    /// large closure moves into its allocation once.
    pub(crate) fn effect_callback(&self, mount_pure: bool, callback: Callback) -> Effect {
        let born = self.inner.next.get();
        self.inner.next.set(born + 1);
        let id = self.inner.effects.borrow_mut().insert(
            born,
            EffectRecord {
                callback,
                dependencies: Default::default(),
                run: 0,
                queued: true,
                stopped: false,
                mount_pure,
            },
        );
        self.inner.queue(id);
        let effect = Effect {
            id,
            runtime: Rc::downgrade(&self.inner),
        };
        self.flush();
        effect
    }
    /// As [`Runtime::effect`], but the first run happens before this returns
    /// even inside a batch or a flush, where `effect` queues it behind what
    /// is running: a value derived by it is readable at once (a computed
    /// made while a component builds, which `on_mount` then reads).
    pub fn effect_now(&self, callback: impl FnMut() + 'static) -> Effect {
        if self.inner.batch_depth.get() == 0 && !self.inner.flushing.get() {
            return self.effect(callback);
        }
        let born = self.inner.next.get();
        self.inner.next.set(born + 1);
        let callback: Callback = Rc::new(RefCell::new(callback));
        let id = self.inner.effects.borrow_mut().insert(
            born,
            EffectRecord {
                callback: callback.clone(),
                dependencies: Default::default(),
                run: 1,
                queued: false,
                stopped: false,
                mount_pure: false,
            },
        );
        // Made before it runs: a first run that panics drops it, so
        // nothing is left subscribed with no handle to dispose of it.
        let effect = Effect {
            id,
            runtime: Rc::downgrade(&self.inner),
        };
        {
            let _active = Active::run(&self.inner.active, id);
            (*callback.borrow_mut())();
        }
        effect
    }
    /// Whether a write now runs no effects before returning: inside a batch,
    /// or a flush, which run them once they end.
    pub fn deferring(&self) -> bool {
        self.inner.batch_depth.get() != 0 || self.inner.flushing.get()
    }
    /// Defers effects until the outermost batch finishes; mutations remain visible.
    pub fn batch<R>(&self, f: impl FnOnce() -> R) -> R {
        self.inner.batch_depth.set(self.inner.batch_depth.get() + 1);
        struct Guard<'a>(&'a Runtime);
        impl Drop for Guard<'_> {
            fn drop(&mut self) {
                self.0
                    .inner
                    .batch_depth
                    .set(self.0.inner.batch_depth.get() - 1);
                if !std::thread::panicking() {
                    self.0.flush();
                }
            }
        }
        let _guard = Guard(self);
        f()
    }
    /// Holds the effects writes wake until the returned guard drops, as a
    /// batch does, with no flush at all when none woke; nothing (`None`)
    /// inside a batch or a flush, where writes already wait. For a removal,
    /// whose destructors may write state its own effects read: those are
    /// gone when the guard drops.
    #[inline(always)]
    pub(crate) fn hold_effects(&self) -> Option<HeldEffects> {
        if self.deferring() {
            return None;
        }
        self.inner.batch_depth.set(self.inner.batch_depth.get() + 1);
        Some(HeldEffects(self.clone()))
    }
    /// The effect whose reads subscribe it now.
    pub(crate) fn active_effect(&self) -> Option<EffectId> {
        self.inner.active.get()
    }
    /// Run `f` with its reads subscribing effect `id`, as if it were part of
    /// its last run: a draw a mount deferred from an effect.
    pub(crate) fn run_as(&self, id: EffectId, f: &dyn Fn()) {
        let _active = Active::run(&self.inner.active, id);
        f();
    }
    /// [`Runtime::batch`], whose flush, if `watched` says it once `f` has
    /// returned, tells `watch` of each observer that isn't `mount_pure` as
    /// it starts (`true`) and ends (`false`, a panic's unwinding included).
    /// For a mount, which keeps what they read of its geometry.
    pub(crate) fn batch_watched<R>(
        &self,
        f: impl FnOnce() -> R,
        mut watch: impl FnMut(EffectId, bool),
        watched: impl Fn() -> bool,
    ) -> R {
        self.inner.batch_depth.set(self.inner.batch_depth.get() + 1);
        struct Guard<'a, F: FnMut(EffectId, bool), P: Fn() -> bool>(&'a Runtime, &'a mut F, P);
        impl<F: FnMut(EffectId, bool), P: Fn() -> bool> Drop for Guard<'_, F, P> {
            fn drop(&mut self) {
                self.0
                    .inner
                    .batch_depth
                    .set(self.0.inner.batch_depth.get() - 1);
                if !std::thread::panicking() {
                    if (self.2)() {
                        self.0.flush_watched(&mut self.1);
                    } else {
                        self.0.flush();
                    }
                }
            }
        }
        let _guard = Guard(self, &mut watch, watched);
        f()
    }
    /// Run observers `ids` again, oldest first, and what they queue, telling
    /// `watch` as `batch_watched` does. One stopped or gone is left alone.
    pub(crate) fn rerun_watched(&self, ids: &[EffectId], mut watch: impl FnMut(EffectId, bool)) {
        self.requeue(ids);
        self.flush_watched(&mut watch);
    }
    /// Queue observers `ids` to run again at the next flush, not now.
    pub(crate) fn requeue(&self, ids: &[EffectId]) {
        let mut effects = self.inner.effects.borrow_mut();
        for id in ids {
            if let Some(record) = effects.get_mut(id).filter(|record| !record.stopped)
                && !std::mem::replace(&mut record.queued, true)
            {
                self.inner.queued.borrow_mut().push(*id);
            }
        }
    }
    /// [`Runtime::flush`], with `watch` told of each observer that isn't
    /// `mount_pure`. Apart, so that `flush` stays as it is.
    #[inline(never)]
    fn flush_watched(&self, watch: &mut impl FnMut(EffectId, bool)) {
        if self.inner.batch_depth.get() != 0 || self.inner.flushing.replace(true) {
            return;
        }
        struct Guard<'a>(&'a Inner);
        impl Drop for Guard<'_> {
            fn drop(&mut self) {
                self.0.active.set(None);
                self.0.flushing.set(false);
            }
        }
        let _guard = Guard(&self.inner);
        let _span = (crate::profile::enabled()
            && !(self.inner.queued.borrow().is_empty() && self.inner.pending.borrow().is_empty()))
        .then(|| crate::profile::span(crate::profile::Phase::Update));
        // As `flush`'s: left unrun by a panic, they are queued again.
        struct Sorted<'a>(&'a Inner, Vec<EffectId>);
        impl Drop for Sorted<'_> {
            fn drop(&mut self) {
                let mut queued = self.0.queued.borrow_mut();
                match queued.is_empty() {
                    true => std::mem::swap(&mut *queued, &mut self.1),
                    false => queued.append(&mut self.1),
                }
            }
        }
        let mut sorted = Sorted(
            &self.inner,
            std::mem::take(&mut *self.inner.queued.borrow_mut()),
        );
        newest_first::<false>(&mut sorted.1, &mut self.inner.order.borrow_mut());
        // Its end told even when it panics.
        struct Watched<'a, F: FnMut(EffectId, bool)>(&'a mut F, EffectId);
        impl<F: FnMut(EffectId, bool)> Drop for Watched<'_, F> {
            fn drop(&mut self) {
                (self.0)(self.1, false);
            }
        }
        loop {
            // The oldest waiting, and whether it is `pending`'s: taken off
            // once `watch` has been told, so that a panic there leaves it
            // queued.
            let (id, from_pending) = {
                let pending = self.inner.pending.borrow();
                match (sorted.1.last(), pending.peek()) {
                    (Some(&old), Some(&Reverse(new))) if new < old => (new, true),
                    (Some(&old), _) => (old, false),
                    (None, Some(&Reverse(new))) => (new, true),
                    (None, None) => break,
                }
            };
            let pure = self
                .inner
                .effects
                .borrow()
                .get(&id)
                .is_some_and(|record| record.mount_pure);
            if !pure {
                watch(id, true);
            }
            if from_pending {
                self.inner.pending.borrow_mut().pop();
            } else {
                sorted.1.pop();
            }
            if pure {
                self.inner.run(id);
                continue;
            }
            let _watched = Watched(watch, id);
            self.inner.run(id);
        }
    }
    /// Run queued observers now, even inside a batch, as React's
    /// `flushSync` does: for code about to read what an observer writes (a
    /// field's text synced from a state a handler just set, before it
    /// selects in it). Inside a flush, they are left to it.
    pub fn settle(&self) {
        struct Restore<'a>(&'a Inner, usize);
        impl Drop for Restore<'_> {
            fn drop(&mut self) {
                self.0.batch_depth.set(self.1);
            }
        }
        let _restore = Restore(&self.inner, self.inner.batch_depth.replace(0));
        self.flush();
    }
    /// Run queued observers. Reentrant writes join the current flush.
    pub fn flush(&self) {
        if self.inner.batch_depth.get() != 0 || self.inner.flushing.replace(true) {
            return;
        }
        struct Guard<'a>(&'a Inner);
        impl Drop for Guard<'_> {
            fn drop(&mut self) {
                self.0.active.set(None);
                self.0.flushing.set(false);
            }
        }
        let _guard = Guard(&self.inner);
        // Only a flush with work: most find nothing queued.
        let _span = (crate::profile::enabled()
            && !(self.inner.queued.borrow().is_empty() && self.inner.pending.borrow().is_empty()))
        .then(|| crate::profile::span(crate::profile::Phase::Update));
        // Sorted newest first, so the oldest is last. Left unrun by a
        // panicking effect, they are queued again.
        struct Sorted<'a>(&'a Inner, Vec<EffectId>);
        impl Drop for Sorted<'_> {
            fn drop(&mut self) {
                let mut queued = self.0.queued.borrow_mut();
                match queued.is_empty() {
                    true => std::mem::swap(&mut *queued, &mut self.1),
                    false => queued.append(&mut self.1),
                }
            }
        }
        let mut sorted = Sorted(
            &self.inner,
            std::mem::take(&mut *self.inner.queued.borrow_mut()),
        );
        newest_first::<true>(&mut sorted.1, &mut self.inner.order.borrow_mut());
        loop {
            let id = {
                let mut pending = self.inner.pending.borrow_mut();
                match (sorted.1.last(), pending.peek()) {
                    (Some(&old), Some(&Reverse(new))) if new < old => {
                        pending.pop();
                        new
                    }
                    (Some(_), _) => sorted.1.pop().expect("an id"),
                    (None, Some(_)) => pending.pop().expect("an id").0,
                    (None, None) => break,
                }
            };
            self.inner.run(id);
        }
    }
    /// `Effect::stop` for each of `effects`, this runtime's looked up at
    /// once.
    pub fn stop_effects<'a>(&self, effects: impl IntoIterator<Item = &'a Effect>) {
        let mut records = self.inner.effects.borrow_mut();
        let mut stopped = 0;
        for effect in effects {
            if !std::ptr::eq(effect.runtime.as_ptr(), Rc::as_ptr(&self.inner)) {
                drop(records);
                effect.stop();
                records = self.inner.effects.borrow_mut();
                continue;
            }
            if let Some(record) = records.get_mut(&effect.id)
                && !std::mem::replace(&mut record.stopped, true)
            {
                record.queued = false;
                stopped += 1;
            }
        }
        self.inner.stopped.set(self.inner.stopped.get() + stopped);
    }
    /// How many effects are alive (not stopped).
    pub fn effect_count(&self) -> usize {
        self.inner.effects.borrow().len() - self.inner.stopped.get()
    }
    /// The runtime's one value of type `T`, made by `init` the first time
    /// it is asked for, so that independent parts of an application (its
    /// `Ui`, a script host) share it without passing it around. It lives as
    /// long as the runtime; hold a [`WeakRuntime`] in it, not a `Runtime`.
    pub fn extension<T: 'static>(&self, init: impl FnOnce(&Runtime) -> T) -> Rc<T> {
        let id = TypeId::of::<T>();
        let found = self
            .inner
            .extensions
            .borrow()
            .iter()
            .find(|(key, _)| *key == id)
            .map(|(_, value)| value.clone());
        if let Some(value) = found {
            return value.downcast().expect("extension type");
        }
        let value = Rc::new(init(self));
        self.inner
            .extensions
            .borrow_mut()
            .push((id, value.clone() as Rc<dyn Any>));
        value
    }
    /// A handle that does not keep the runtime alive.
    pub fn downgrade(&self) -> WeakRuntime {
        WeakRuntime(Rc::downgrade(&self.inner))
    }
}
/// A [`Runtime`] that may have been dropped.
#[derive(Clone, Default)]
pub struct WeakRuntime(Weak<Inner>);
impl WeakRuntime {
    /// The runtime, if it still exists.
    pub fn upgrade(&self) -> Option<Runtime> {
        self.0.upgrade().map(|inner| Runtime { inner })
    }
}

/// A subscription's ownership is explicit: dropping it removes all dependencies.
pub struct Effect {
    id: EffectId,
    runtime: Weak<Inner>,
}
impl Effect {
    /// Its number, as traces name it (`zgx::trace`, with the `trace`
    /// feature).
    pub fn number(&self) -> u64 {
        self.id
    }
    /// Never run again, from now: no write queues it, nor does a queued
    /// run happen. What it holds (its closure and what that captured, its
    /// subscriptions) goes when the handle drops, which may be later, off
    /// the frame (`Ui::remove` frees a large subtree's over frames).
    pub fn stop(&self) {
        let Some(runtime) = self.runtime.upgrade() else {
            return;
        };
        if let Some(record) = runtime.effects.borrow_mut().get_mut(&self.id)
            && !std::mem::replace(&mut record.stopped, true)
        {
            record.queued = false;
            runtime.stopped.set(runtime.stopped.get() + 1);
        }
    }
    /// A handle that runs this effect early, while it lives.
    pub fn refresher(&self) -> Refresh {
        Refresh {
            id: self.id,
            runtime: self.runtime.clone(),
        }
    }
}
/// Runs an effect now if a write has queued it (`Effect::refresher`): a
/// value an effect derives (a computed) is current when read inside a
/// batch, or in a handler, rather than once the outermost batch ends. It
/// keeps neither the effect nor the runtime alive.
#[derive(Clone)]
pub struct Refresh {
    id: EffectId,
    runtime: Weak<Inner>,
}
impl Refresh {
    /// Run the effect if it is queued and not running (an effect reading
    /// what it derives sees its last value). Returns whether it ran.
    pub fn refresh(&self) -> bool {
        let Some(runtime) = self.runtime.upgrade() else {
            return false;
        };
        let running = runtime
            .effects
            .borrow()
            .get(&self.id)
            .is_none_or(|record| record.callback.try_borrow_mut().is_err());
        !running && runtime.run(self.id)
    }
}
impl Drop for Effect {
    fn drop(&mut self) {
        // While tracing, its name and pending cause go with it.
        #[cfg(feature = "trace")]
        if crate::trace::enabled() {
            crate::trace::forget(self.id);
        }
        if let Some(runtime) = self.runtime.upgrade() {
            let record = runtime.effects.borrow_mut().remove(&self.id);
            if let Some(record) = record {
                if record.stopped {
                    runtime.stopped.set(runtime.stopped.get() - 1);
                }
                for dependency in record.dependencies {
                    if let Some(dependency) = dependency.source.upgrade() {
                        dependency.unsubscribe(self.id);
                    }
                }
            }
        }
    }
}

struct SignalInner<T> {
    value: RefCell<T>,
    /// Writes that changed the value (`Signal::version`).
    version: Cell<u64>,
    subscribers: RefCell<IdSet>,
    runtime: Weak<Inner>,
}
impl<T> Dependency for SignalInner<T> {
    fn unsubscribe(&self, id: EffectId) {
        self.subscribers.borrow_mut().remove(&id);
    }
}
/// Effects held by [`Runtime::hold_effects`] until it drops.
pub(crate) struct HeldEffects(Runtime);
impl Drop for HeldEffects {
    fn drop(&mut self) {
        let inner = &self.0.inner;
        inner.batch_depth.set(inner.batch_depth.get() - 1);
        if !std::thread::panicking()
            && (!inner.queued.borrow().is_empty() || !inner.pending.borrow().is_empty())
        {
            self.0.flush();
        }
    }
}
/// A reactive value. Reading it in an effect subscribes the effect, and a write
/// that changes it re-runs its subscribers. Clones share the value.
pub struct Signal<T> {
    inner: Rc<SignalInner<T>>,
}
impl<T> Clone for Signal<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}
/// A [`Signal`] that may have been dropped (`Signal::downgrade`).
pub struct WeakSignal<T>(Weak<SignalInner<T>>);
impl<T> Clone for WeakSignal<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<T> WeakSignal<T> {
    /// The signal, if a handle to it still lives.
    pub fn upgrade(&self) -> Option<Signal<T>> {
        self.0.upgrade().map(|inner| Signal { inner })
    }
    /// Whether a handle to the signal still lives.
    pub fn is_alive(&self) -> bool {
        self.0.strong_count() > 0
    }
}
/// The run mark of a subscription kept whatever its effect reads
/// (`Signal::subscribe`).
const KEPT: u64 = u64::MAX;
impl<T: 'static> Signal<T> {
    /// Whether `other` is this signal (a clone of it), not just equal.
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
    fn track(&self) {
        self.track_as(false);
    }
    /// Subscribe the running effect for as long as it lives, whether or not
    /// later runs read the signal: for an effect that reads it on most
    /// runs, untracked, which saves tracking it each time.
    pub(crate) fn subscribe(&self) {
        self.track_as(true);
    }
    #[inline(always)]
    fn track_as(&self, kept: bool) {
        let Some(runtime) = self.inner.runtime.upgrade() else {
            return;
        };
        let Some(id) = runtime.active.get() else {
            return;
        };
        let mut effects = runtime.effects.borrow_mut();
        let Some(record) = effects.get_mut(&id) else {
            // A running callback can dispose its own owner. Later reads must
            // not leave subscriptions that no Effect can remove.
            return;
        };
        let key = Rc::as_ptr(&self.inner).cast::<()>();
        let run = if kept { KEPT } else { record.run };
        // Effects read few signals; a linear scan beats any set lookup.
        if let Some(existing) = record.dependencies.iter_mut().find(|d| d.key == key) {
            if existing.run != KEPT {
                existing.run = run;
            }
        } else if self.inner.subscribers.borrow_mut().insert(id) {
            let dependency: Rc<dyn Dependency> = self.inner.clone();
            record.dependencies.push(Subscription {
                source: Rc::downgrade(&dependency),
                key,
                run,
            });
        }
    }
    /// Whether any effect reads it: for a host dropping data nothing shows.
    pub fn is_observed(&self) -> bool {
        !self.inner.subscribers.borrow().is_empty()
    }
    /// Whether both handles are the same signal.
    pub fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
    /// Identifies this signal while any handle to it lives.
    pub fn addr(&self) -> usize {
        Rc::as_ptr(&self.inner).cast::<()>() as usize
    }
    /// A handle that does not keep the signal (or its value) alive.
    pub fn downgrade(&self) -> WeakSignal<T> {
        WeakSignal(Rc::downgrade(&self.inner))
    }
    /// Call `f` with the value, subscribing the running effect.
    #[inline(always)]
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        self.track();
        f(&self.inner.value.borrow())
    }
    /// How many writes have changed it: the same number means the same
    /// value, so a reader that kept one need not compare it again.
    pub fn version(&self) -> u64 {
        self.inner.version.get()
    }
    /// Call `f` with the value, without subscribing.
    pub fn with_untracked<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        f(&self.inner.value.borrow())
    }
    /// A clone of the value, subscribing the running effect.
    pub fn get(&self) -> T
    where
        T: Clone,
    {
        self.with(Clone::clone)
    }
    fn notify(&self) {
        if let Some(inner) = self.inner.runtime.upgrade() {
            {
                let mut effects = inner.effects.borrow_mut();
                let flushing = inner.flushing.get();
                let (mut queued, mut pending) =
                    (inner.queued.borrow_mut(), inner.pending.borrow_mut());
                #[cfg(feature = "trace")]
                let mut caused: Option<Vec<EffectId>> = crate::trace::enabled().then(Vec::new);
                for id in self.inner.subscribers.borrow().iter() {
                    if let Some(record) = effects.get_mut(id)
                        && !record.stopped
                        && !std::mem::replace(&mut record.queued, true)
                    {
                        #[cfg(feature = "trace")]
                        if let Some(caused) = &mut caused {
                            caused.push(*id);
                        }
                        match flushing {
                            true => pending.push(Reverse(*id)),
                            false => queued.push(*id),
                        }
                    }
                }
                #[cfg(feature = "trace")]
                if let Some(caused) = caused {
                    crate::trace::wrote(self.addr(), &caused);
                }
            }
            Runtime { inner }.flush();
        }
    }
    /// Returns whether the value changed.
    pub fn set(&self, value: T) -> bool
    where
        T: PartialEq,
    {
        {
            let mut old = self.inner.value.borrow_mut();
            if *old == value {
                return false;
            }
            *old = value;
        }
        self.inner
            .version
            .set(self.inner.version.get().wrapping_add(1));
        self.notify();
        true
    }
    /// Change the value in place with `f`, which says whether it changed
    /// anything; what reads it updates if so. For a large value written
    /// often (a long list's rows), which `update` would clone whole.
    pub fn mutate(&self, f: impl FnOnce(&mut T) -> bool) -> bool {
        let changed = f(&mut self.inner.value.borrow_mut());
        if changed {
            self.inner
                .version
                .set(self.inner.version.get().wrapping_add(1));
            self.notify();
        }
        changed
    }
    /// Change a clone of the value with `f` and set it; returns whether it changed.
    pub fn update(&self, f: impl FnOnce(&mut T)) -> bool
    where
        T: PartialEq + Clone,
    {
        let mut next = self.with_untracked(Clone::clone);
        f(&mut next);
        self.set(next)
    }
}

/// Lexically scoped, typed service storage for a future compiler's `provide` nodes.
#[derive(Clone, Default)]
pub struct ServiceScope {
    parent: Option<Rc<ServiceScope>>,
    values: HashMap<TypeId, Rc<dyn Any>>,
}
impl ServiceScope {
    /// An empty scope with no parent.
    pub fn new() -> Self {
        Self::default()
    }
    /// A child scope: it sees this scope's values as they are now, and what it
    /// provides itself shadows them.
    pub fn child(&self) -> Self {
        Self {
            parent: Some(Rc::new(self.clone())),
            values: HashMap::new(),
        }
    }
    /// Store `value` as this scope's `T`, replacing any; returns it shared.
    pub fn provide<T: 'static>(&mut self, value: T) -> Rc<T> {
        let value = Rc::new(value);
        self.values.insert(TypeId::of::<T>(), value.clone());
        value
    }
    /// The nearest `T`: this scope's, else its parent's, and so on up.
    pub fn get<T: 'static>(&self) -> Option<Rc<T>> {
        self.values
            .get(&TypeId::of::<T>())
            .cloned()
            .and_then(|value| value.downcast().ok())
            .or_else(|| self.parent.as_ref().and_then(|parent| parent.get()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Queued ids come out newest first, as a sort leaves them, whether
    /// they are close together (a bitmap orders them) or far apart.
    #[test]
    fn queued_effects_are_ordered_newest_first() {
        let mut seed = 0x2545_f491_4f6c_dd1d_u64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let mut order = Order::default();
        for (count, spread) in [(0, 1), (1, 1), (31, 40), (32, 32), (900, 1000), (900, 5000)]
            .into_iter()
            .chain([(64, 64 * 33), (500, 1 << 38)])
        {
            let base = next() % 1_000_000;
            let mut ids: Vec<EffectId> = Vec::new();
            while ids.len() < count {
                // Born close together or far apart, in any slot.
                let born = base + next() % spread;
                let id = born << SLOT_BITS | next() & SLOT_MASK;
                if !ids.iter().any(|&other| other >> SLOT_BITS == born) {
                    ids.push(id);
                }
            }
            let mut sorted = ids.clone();
            sorted.sort_unstable_by(|a, b| b.cmp(a));
            newest_first::<true>(&mut ids, &mut order);
            assert_eq!(ids, sorted, "{count} ids over {spread}");
            // Queued as they were made: oldest first.
            ids.reverse();
            newest_first::<false>(&mut ids, &mut order);
            assert_eq!(ids, sorted, "{count} ascending ids over {spread}");
        }
    }
    #[test]
    fn a_signal_version_counts_the_writes_that_changed_it() {
        let runtime = Runtime::new();
        let signal = runtime.signal(1);
        assert_eq!(signal.version(), 0);
        signal.set(1);
        signal.update(|value| *value = 1);
        signal.mutate(|_| false);
        assert_eq!(signal.version(), 0, "writes changing nothing keep it");
        signal.set(2);
        signal.mutate(|value| {
            *value = 3;
            true
        });
        assert_eq!(signal.version(), 2);
    }
    #[test]
    fn a_refreshed_effect_derives_its_value_inside_a_batch_and_runs_once() {
        let runtime = Runtime::new();
        let source = runtime.signal(1);
        let derived = runtime.signal(0);
        let runs = Rc::new(Cell::new(0));
        let effect = runtime.effect({
            let (source, derived, runs) = (source.clone(), derived.clone(), runs.clone());
            move || {
                runs.set(runs.get() + 1);
                derived.set(source.get() * 10);
            }
        });
        let refresh = effect.refresher();
        assert!(!refresh.refresh(), "nothing queued");
        runtime.batch(|| {
            source.set(2);
            assert_eq!(derived.get(), 10, "deferred in the batch");
            assert!(refresh.refresh());
            assert_eq!(derived.get(), 20, "current once refreshed");
            assert!(!refresh.refresh());
        });
        // The batch's flush finds it already run.
        assert_eq!(runs.get(), 2);
        drop(effect);
        assert!(!refresh.refresh(), "a disposed effect does nothing");
    }
    #[test]
    fn untracked_nested_and_caught_panic_restore_outer_dependencies() {
        let runtime = Runtime::new();
        let before = runtime.signal(0);
        let hidden = runtime.signal(0);
        let after = runtime.signal(0);
        let runs = Rc::new(Cell::new(0));
        let _effect = runtime.effect({
            let runtime = runtime.clone();
            let before = before.clone();
            let hidden = hidden.clone();
            let after = after.clone();
            let runs = runs.clone();
            move || {
                runs.set(runs.get() + 1);
                before.get();
                assert_eq!(
                    runtime.untracked(|| runtime.untracked(|| hidden.get())),
                    hidden.with_untracked(|v| *v)
                );
                let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    runtime.untracked(|| {
                        runtime.untracked(|| hidden.get());
                        panic!("intentional construction panic");
                    });
                }));
                assert!(caught.is_err());
                after.get();
            }
        });
        hidden.set(1);
        assert_eq!(runs.get(), 1);
        before.set(1);
        after.set(1);
        assert_eq!(runs.get(), 3);
    }

    #[test]
    fn untracked_component_effect_tracks_its_own_reads() {
        let runtime = Runtime::new();
        let branch = runtime.signal(0);
        let child_value = runtime.signal(0);
        let branch_runs = Rc::new(Cell::new(0));
        let child_runs = Rc::new(Cell::new(0));
        let child = Rc::new(RefCell::new(None));
        let _branch_effect = runtime.effect({
            let runtime = runtime.clone();
            let branch = branch.clone();
            let child_value = child_value.clone();
            let branch_runs = branch_runs.clone();
            let child_runs = child_runs.clone();
            let child = child.clone();
            move || {
                branch.get();
                branch_runs.set(branch_runs.get() + 1);
                runtime.untracked(|| {
                    child_value.get();
                    *child.borrow_mut() = Some(runtime.effect({
                        let child_value = child_value.clone();
                        let child_runs = child_runs.clone();
                        move || {
                            child_value.get();
                            child_runs.set(child_runs.get() + 1);
                        }
                    }));
                });
            }
        });
        child_value.set(1);
        assert_eq!(branch_runs.get(), 1);
        assert_eq!(child_runs.get(), 2);
        branch.set(1);
        assert_eq!(branch_runs.get(), 2);
        assert_eq!(child_runs.get(), 3);
    }
    #[test]
    fn subscriptions_kept_without_reading() {
        let runtime = Runtime::new();
        let kept = runtime.signal(0);
        let read = runtime.signal(0);
        let runs = Rc::new(Cell::new(0));
        let effect = runtime.effect({
            let (kept, read, runs) = (kept.clone(), read.clone(), runs.clone());
            move || {
                runs.set(runs.get() + 1);
                if runs.get() == 1 {
                    kept.subscribe();
                }
                read.get();
            }
        });
        // Later runs do not read `kept`, and stay subscribed to it.
        read.set(1);
        kept.set(1);
        kept.set(2);
        assert_eq!(runs.get(), 4);
        drop(effect);
        kept.set(3);
        assert_eq!(runs.get(), 4);
        assert!(!kept.is_observed());
    }
    #[test]
    fn batching_equality_dynamic_dependencies_and_disposal() {
        let runtime = Runtime::new();
        let branch = runtime.signal(true);
        let a = runtime.signal(0);
        let b = runtime.signal(0);
        let runs = Rc::new(Cell::new(0));
        let effect = runtime.effect({
            let branch = branch.clone();
            let a = a.clone();
            let b = b.clone();
            let runs = runs.clone();
            move || {
                runs.set(runs.get() + 1);
                if branch.get() {
                    a.get();
                } else {
                    b.get();
                }
            }
        });
        runtime.batch(|| {
            a.set(1);
            a.set(2);
            a.set(2);
            b.set(8);
        });
        assert_eq!(runs.get(), 2);
        branch.set(false);
        a.set(3);
        assert_eq!(runs.get(), 3);
        b.set(9);
        assert_eq!(runs.get(), 4);
        drop(effect);
        b.set(10);
        assert_eq!(runs.get(), 4);
        assert_eq!(runtime.effect_count(), 0);
    }
    #[test]
    fn chained_updates_and_panic_recovery() {
        let runtime = Runtime::new();
        let a = runtime.signal(0);
        let b = runtime.signal(0);
        let _effect = runtime.effect({
            let a = a.clone();
            let b = b.clone();
            move || {
                b.set(a.get() * 2);
            }
        });
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            runtime.batch(|| {
                a.set(4);
                panic!("test");
            })
        }));
        runtime.flush();
        assert_eq!(b.get(), 8);
    }
    #[test]
    fn self_disposed_effect_cannot_subscribe_after_disposal() {
        let runtime = Runtime::new();
        let trigger = runtime.signal(false);
        let late = runtime.signal(0);
        let slot = Rc::new(RefCell::new(None::<Effect>));
        let effect = runtime.effect({
            let trigger = trigger.clone();
            let late = late.clone();
            let slot = slot.clone();
            move || {
                if trigger.get() {
                    drop(slot.borrow_mut().take());
                    late.get();
                }
            }
        });
        *slot.borrow_mut() = Some(effect);
        trigger.set(true);
        assert_eq!(runtime.effect_count(), 0);
        assert!(trigger.inner.subscribers.borrow().is_empty());
        assert!(late.inner.subscribers.borrow().is_empty());
        runtime.batch(|| {
            late.set(1);
            assert!(runtime.inner.queued.borrow().is_empty());
            assert!(runtime.inner.pending.borrow().is_empty());
        });
    }

    #[test]
    fn effects_run_oldest_first_whether_queued_before_or_during_a_flush() {
        let runtime = Runtime::new();
        let order = Rc::new(RefCell::new(Vec::new()));
        let (a, b, c) = (runtime.signal(0), runtime.signal(0), runtime.signal(0));
        let log = |name: &'static str| {
            let order = order.clone();
            move || order.borrow_mut().push(name)
        };
        // Made in this order: the oldest first.
        let _first = runtime.effect({
            let (a, log) = (a.clone(), log("first"));
            move || {
                a.get();
                log();
            }
        });
        let _second = runtime.effect({
            let (a, b, log) = (a.clone(), b.clone(), log("second"));
            move || {
                if b.get() > 0 {
                    a.set(b.get());
                }
                log();
            }
        });
        let _third = runtime.effect({
            let (c, log) = (c.clone(), log("third"));
            move || {
                c.get();
                log();
            }
        });
        order.borrow_mut().clear();
        // The third queued first and the second then; the second queues the
        // first as it runs, which still runs before the third.
        runtime.batch(|| {
            c.set(1);
            b.set(1);
        });
        assert_eq!(*order.borrow(), ["second", "first", "third"]);
    }

    #[test]
    fn effects_left_by_a_panicking_one_run_at_the_next_flush() {
        let runtime = Runtime::new();
        let ran = Rc::new(Cell::new(0));
        let (a, b) = (runtime.signal(0), runtime.signal(0));
        let _panics = runtime.effect({
            let a = a.clone();
            move || assert_ne!(a.get(), 1, "on purpose")
        });
        let _after = runtime.effect({
            let (b, ran) = (b.clone(), ran.clone());
            move || {
                b.get();
                ran.set(ran.get() + 1);
            }
        });
        ran.set(0);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            runtime.batch(|| {
                a.set(1);
                b.set(1);
            })
        }));
        assert!(result.is_err());
        assert_eq!(ran.get(), 0);
        runtime.flush();
        assert_eq!(ran.get(), 1);
    }

    #[test]
    fn providers_shadow_without_mutating_parent() {
        let mut root = ServiceScope::new();
        root.provide(1_u32);
        let mut child = root.child();
        child.provide(2_u32);
        assert_eq!(*root.get::<u32>().unwrap(), 1);
        assert_eq!(*child.get::<u32>().unwrap(), 2);
    }

    #[test]
    fn dropping_effect_releases_captured_runtime_and_data() {
        let runtime = Runtime::new();
        let runtime_weak = Rc::downgrade(&runtime.inner);
        let payload = Rc::new(vec![0_u8; 1024]);
        let payload_weak = Rc::downgrade(&payload);
        let effect = runtime.effect({
            let runtime = runtime.clone();
            move || {
                assert_eq!(payload.len(), 1024);
                assert_eq!(runtime.effect_count(), 1);
            }
        });
        drop(runtime);
        assert!(runtime_weak.upgrade().is_some());
        drop(effect);
        assert!(runtime_weak.upgrade().is_none());
        assert!(payload_weak.upgrade().is_none());
    }

    #[test]
    fn borrowed_reads_need_no_clone_and_observer_panic_recovers() {
        struct NonClone(Vec<u8>);
        let runtime = Runtime::new();
        let data = runtime.signal(NonClone(vec![0; 1024]));
        assert_eq!(data.with(|value| value.0.len()), 1024);
        let value = runtime.signal(0);
        let observed = Rc::new(Cell::new(0));
        let _effect = runtime.effect({
            let value = value.clone();
            let observed = observed.clone();
            move || {
                let current = value.get();
                assert_ne!(current, 1, "intentional observer panic");
                observed.set(current);
            }
        });
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| value.set(1)));
        assert!(result.is_err());
        value.set(2);
        assert_eq!(observed.get(), 2);
    }
}

#[cfg(test)]
mod watched_tests {
    use super::*;
    #[test]
    fn a_watched_flush_tells_of_each_observer_as_it_starts_and_ends() {
        let runtime = Runtime::new();
        let log = Rc::new(RefCell::new(Vec::new()));
        let value = runtime.signal(0);
        let read = value.clone();
        let out = log.clone();
        let watched = runtime.effect(move || {
            read.get();
            out.borrow_mut().push("ran");
        });
        let read = value.clone();
        let out = log.clone();
        let pure = runtime.effect_marked(true, move || {
            read.get();
            out.borrow_mut().push("pure");
        });
        log.borrow_mut().clear();
        let id = watched.number();
        runtime.batch_watched(
            || value.set(1),
            |effect, starts| {
                assert_eq!(effect, id);
                log.borrow_mut()
                    .push(if starts { "starts" } else { "ends" });
            },
            || true,
        );
        assert_eq!(*log.borrow(), ["starts", "ran", "ends", "pure"]);
        log.borrow_mut().clear();
        runtime.rerun_watched(&[id], |_, starts| {
            log.borrow_mut()
                .push(if starts { "starts" } else { "ends" });
        });
        assert_eq!(*log.borrow(), ["starts", "ran", "ends"]);
        drop((watched, pure));
    }
    #[test]
    fn a_panicking_watch_keeps_the_selected_observer_queued() {
        let runtime = Runtime::new();
        let runs = Rc::new(Cell::new(0));
        let value = runtime.signal(0);
        let read = value.clone();
        let count = runs.clone();
        let _effect = runtime.effect(move || {
            read.get();
            count.set(count.get() + 1);
        });
        assert_eq!(runs.get(), 1);
        let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            runtime.batch_watched(|| value.set(1), |_, _| panic!("watch probe"), || true);
        }));
        assert!(failed.is_err());
        assert_eq!(runs.get(), 1);
        runtime.flush();
        assert_eq!(runs.get(), 2);
    }
}
