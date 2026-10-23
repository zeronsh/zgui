//! Single-threaded fine-grained reactivity. Effects subscribe only to values read.
use std::{
    any::{Any, TypeId},
    cell::{Cell, RefCell},
    collections::{BTreeSet, HashMap, HashSet},
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
type IdMap<V> = HashMap<EffectId, V, std::hash::BuildHasherDefault<IdHasher>>;
/// A signal's subscribers, in no order: `pending` orders what runs. A set
/// most nodes' effects join (the inherited typography) is removed from
/// one by one as they go, in constant time.
type IdSet = HashSet<EffectId, std::hash::BuildHasherDefault<IdHasher>>;
#[derive(Default)]
struct Inner {
    effects: RefCell<IdMap<EffectRecord>>,
    pending: RefCell<BTreeSet<EffectId>>,
    active: Cell<Option<EffectId>>,
    next: Cell<EffectId>,
    batch_depth: Cell<usize>,
    flushing: Cell<bool>,
    /// One value per type that belongs to this runtime (`Runtime::extension`).
    extensions: RefCell<Vec<(TypeId, Rc<dyn Any>)>>,
}

/// An explicit runtime avoids global state and supports multiple independent UI roots.
#[derive(Clone, Default)]
pub struct Runtime {
    inner: Rc<Inner>,
}
impl Runtime {
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
    pub fn signal<T: 'static>(&self, value: T) -> Signal<T> {
        Signal {
            inner: Rc::new(SignalInner {
                value: RefCell::new(value),
                subscribers: RefCell::default(),
                runtime: Rc::downgrade(&self.inner),
            }),
        }
    }
    /// Runs immediately. Keep the returned handle alive to keep the subscription.
    pub fn effect(&self, callback: impl FnMut() + 'static) -> Effect {
        let id = self.inner.next.get();
        self.inner
            .next
            .set(id.checked_add(1).expect("effect identifiers exhausted"));
        self.inner.effects.borrow_mut().insert(
            id,
            EffectRecord {
                callback: Rc::new(RefCell::new(callback)),
                dependencies: Default::default(),
                run: 0,
            },
        );
        self.inner.pending.borrow_mut().insert(id);
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
        let id = self.inner.next.get();
        self.inner
            .next
            .set(id.checked_add(1).expect("effect identifiers exhausted"));
        let callback: Callback = Rc::new(RefCell::new(callback));
        self.inner.effects.borrow_mut().insert(
            id,
            EffectRecord {
                callback: callback.clone(),
                dependencies: Default::default(),
                run: 1,
            },
        );
        let previous = self.inner.active.replace(Some(id));
        (*callback.borrow_mut())();
        self.inner.active.set(previous);
        Effect {
            id,
            runtime: Rc::downgrade(&self.inner),
        }
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
        loop {
            let Some(id) = self.inner.pending.borrow_mut().pop_first() else {
                break;
            };
            let callback = {
                let mut effects = self.inner.effects.borrow_mut();
                effects.get_mut(&id).map(|record| {
                    record.run = record.run.wrapping_add(1);
                    record.callback.clone()
                })
            };
            let Some(callback) = callback else {
                continue;
            };
            self.inner.active.set(Some(id));
            (*callback.borrow_mut())();
            self.inner.active.set(None);
            // Drop subscriptions this run no longer read. The record may be
            // gone if the callback disposed its own effect.
            let stale: Vec<Weak<dyn Dependency>> = {
                let mut effects = self.inner.effects.borrow_mut();
                let Some(record) = effects.get_mut(&id) else {
                    continue;
                };
                let run = record.run;
                // `KEPT` is above every run: read this run, or kept.
                if record.dependencies.iter().all(|d| d.run >= run) {
                    continue;
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
        }
    }
    pub fn effect_count(&self) -> usize {
        self.inner.effects.borrow().len()
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
    pub fn upgrade(&self) -> Option<Runtime> {
        self.0.upgrade().map(|inner| Runtime { inner })
    }
}

/// A subscription's ownership is explicit: dropping it removes all dependencies.
pub struct Effect {
    id: EffectId,
    runtime: Weak<Inner>,
}
impl Drop for Effect {
    fn drop(&mut self) {
        if let Some(runtime) = self.runtime.upgrade() {
            let record = runtime.effects.borrow_mut().remove(&self.id);
            if let Some(record) = record {
                for dependency in record.dependencies {
                    if let Some(dependency) = dependency.source.upgrade() {
                        dependency.unsubscribe(self.id);
                    }
                }
            }
            runtime.pending.borrow_mut().remove(&self.id);
        }
    }
}

struct SignalInner<T> {
    value: RefCell<T>,
    subscribers: RefCell<IdSet>,
    runtime: Weak<Inner>,
}
impl<T> Dependency for SignalInner<T> {
    fn unsubscribe(&self, id: EffectId) {
        self.subscribers.borrow_mut().remove(&id);
    }
}
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
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        self.track();
        f(&self.inner.value.borrow())
    }
    pub fn with_untracked<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        f(&self.inner.value.borrow())
    }
    pub fn get(&self) -> T
    where
        T: Clone,
    {
        self.with(Clone::clone)
    }
    fn notify(&self) {
        if let Some(inner) = self.inner.runtime.upgrade() {
            inner
                .pending
                .borrow_mut()
                .extend(self.inner.subscribers.borrow().iter().copied());
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
        self.notify();
        true
    }
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
    pub fn new() -> Self {
        Self::default()
    }
    pub fn child(&self) -> Self {
        Self {
            parent: Some(Rc::new(self.clone())),
            values: HashMap::new(),
        }
    }
    pub fn provide<T: 'static>(&mut self, value: T) -> Rc<T> {
        let value = Rc::new(value);
        self.values.insert(TypeId::of::<T>(), value.clone());
        value
    }
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
            assert!(runtime.inner.pending.borrow().is_empty());
        });
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
