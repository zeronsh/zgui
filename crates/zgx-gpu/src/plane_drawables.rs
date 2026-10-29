//! A GPU view plane's drawables, waited for off the main thread.
//!
//! A plane's next frame is drawn when its layer has a drawable free, as an
//! app waiting in `nextDrawable` (wgpu's surfaces) would draw it. Paced by
//! the drawables' presented handlers instead, which come later than the
//! drawable is free, two frames on their way held an M1 Max's plane to 80
//! frames a second at 120 Hz (its frames 25 ms on their way). Here a thread
//! of the plane's own does the waiting, so the main thread never does:
//! asked for a drawable (`Drawables::want`), it waits in `nextDrawable`,
//! hands the drawable over and wakes the host, which draws the frame then
//! (`take`).
//!
//! Idle, it does nothing: the thread is made the first time a frame is
//! owed, and waits on a condition variable (no timer) until the next one
//! is. A drawable crosses to the main thread once, under a lock, and only
//! the main thread touches it after.
use objc2::{rc::Retained, runtime::ProtocolObject};
use objc2_quartz_core::{CAMetalDrawable, CAMetalLayer};
use std::sync::{
    Arc, Condvar, Mutex, MutexGuard,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

type Drawable = Retained<ProtocolObject<dyn CAMetalDrawable>>;

/// Moved to the drawables' thread, or back.
struct Sendable<T>(T);
// SAFETY: A CAMetalLayer's `nextDrawable` may be called from any thread,
// and a drawable is handed between threads under `Shared::state`, touched
// by one thread at a time: the waiting thread until it hands it over, the
// main thread after.
unsafe impl<T> Send for Sendable<T> {}

#[derive(Default)]
struct State {
    /// A frame is owed: the thread waits for a drawable.
    wanted: bool,
    /// The drawable it got, not yet taken.
    ready: Option<Sendable<Drawable>>,
    /// In `nextDrawable` now.
    waiting: bool,
    stop: bool,
}
#[derive(Default)]
struct Shared {
    state: Mutex<State>,
    changed: Condvar,
    /// A wake sent and not yet answered (`take`): one a drawable.
    woken: AtomicBool,
    /// Drawables handed over so far (the present log).
    handed: AtomicU64,
}
impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// A plane layer's drawables, waited for on a thread of their own.
pub(crate) struct Drawables {
    layer: Retained<CAMetalLayer>,
    wake: Option<crate::PlaneWaker>,
    shared: Arc<Shared>,
    thread: Option<std::thread::JoinHandle<()>>,
    /// No thread could be made: drawables are taken as frames are drawn.
    failed: bool,
}
impl Drawables {
    /// No thread yet: the first `want` makes it.
    pub(crate) fn new(layer: Retained<CAMetalLayer>, wake: Option<crate::PlaneWaker>) -> Self {
        Self {
            layer,
            wake,
            shared: Default::default(),
            thread: None,
            failed: false,
        }
    }
    /// The drawable handed over, if one is, taken: the main thread's now.
    pub(crate) fn take(&self) -> Option<Drawable> {
        let mut state = self.shared.lock();
        // Answered with the drawable it was for, under the same lock.
        self.shared.woken.store(false, Ordering::Release);
        state.ready.take().map(|drawable| drawable.0)
    }
    /// Whether a drawable waits to be taken.
    pub(crate) fn ready(&self) -> bool {
        self.shared.lock().ready.is_some()
    }
    /// A frame is owed: the thread (made the first time) waits for the next
    /// drawable and wakes the host with it. Nothing if one waits already.
    /// Whether it does: not if no thread could be made, the caller then
    /// taking a drawable itself.
    pub(crate) fn want(&mut self) -> bool {
        if self.failed {
            return false;
        }
        if self.thread.is_none() {
            let layer = Sendable(self.layer.clone());
            let (shared, wake) = (self.shared.clone(), self.wake.clone());
            match std::thread::Builder::new()
                .name("zgx plane drawables".into())
                .spawn(move || wait(layer, &shared, wake.as_ref()))
            {
                Ok(thread) => self.thread = Some(thread),
                Err(error) => {
                    eprintln!(
                        "zgx: no thread for a GPU view's drawables ({error}): it waits for them as frames are drawn"
                    );
                    self.failed = true;
                    return false;
                }
            }
        }
        let mut state = self.shared.lock();
        if state.ready.is_none() && !state.wanted {
            state.wanted = true;
            self.shared.changed.notify_one();
        }
        true
    }
    /// No frame is owed any more (its layer hidden): the thread parks once
    /// its wait, if any, ends.
    pub(crate) fn cancel(&self) {
        self.shared.lock().wanted = false;
    }
    /// Drawables handed over so far.
    pub(crate) fn handed(&self) -> u64 {
        self.shared.handed.load(Ordering::Relaxed)
    }
    #[cfg(test)]
    fn thread_done(&self) -> bool {
        self.thread
            .as_ref()
            .is_none_or(|thread| thread.is_finished())
    }
}
impl Drop for Drawables {
    /// The thread stops: joined if it is parked, else left to end as its
    /// wait does (holding nothing of the window's but the layer, and
    /// dropping the drawable it gets), so the main thread never waits.
    fn drop(&mut self) {
        let waiting = {
            let mut state = self.shared.lock();
            state.stop = true;
            state.ready = None;
            state.waiting
        };
        self.shared.changed.notify_one();
        if let Some(thread) = self.thread.take()
            && !waiting
        {
            let _ = thread.join();
        }
    }
}

/// The drawables' thread: parked until a drawable is wanted, then waiting
/// in `nextDrawable` for one, handing it over and waking the host.
fn wait(
    layer: Sendable<Retained<CAMetalLayer>>,
    shared: &Shared,
    wake: Option<&crate::PlaneWaker>,
) {
    loop {
        {
            let mut state = shared.lock();
            while !state.stop && !(state.wanted && state.ready.is_none()) {
                state = shared
                    .changed
                    .wait(state)
                    .unwrap_or_else(|e| e.into_inner());
            }
            if state.stop {
                return;
            }
            state.waiting = true;
        }
        // Off the main thread: its autoreleased objects are this pool's.
        let drawable = objc2::rc::autoreleasepool(|_| layer.0.nextDrawable());
        let mut state = shared.lock();
        state.waiting = false;
        if state.stop {
            return;
        }
        // None: Core Animation gave none in its time (the layer hidden
        // meanwhile); still wanted, it waits again. No longer wanted, the
        // drawable goes back to the pool.
        if let Some(drawable) = drawable
            && state.wanted
        {
            state.ready = Some(Sendable(drawable));
            state.wanted = false;
            drop(state);
            shared.handed.fetch_add(1, Ordering::Relaxed);
            if !shared.woken.swap(true, Ordering::AcqRel)
                && let Some(wake) = wake
            {
                wake();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use objc2_core_foundation::CGSize;
    use objc2_metal::{MTLCreateSystemDefaultDevice, MTLPixelFormat};
    use std::sync::atomic::AtomicUsize;
    use std::time::{Duration, Instant};

    fn layer() -> Option<Retained<CAMetalLayer>> {
        let device = MTLCreateSystemDefaultDevice()?;
        let layer = CAMetalLayer::new();
        layer.setDevice(Some(&device));
        layer.setPixelFormat(MTLPixelFormat::BGRA8Unorm);
        layer.setMaximumDrawableCount(2);
        layer.setDrawableSize(CGSize::new(16., 16.));
        Some(layer)
    }
    /// Whether the thread waits in nextDrawable (rather than getting one).
    fn waits(drawables: &Drawables) -> bool {
        assert!(until(|| {
            let state = drawables.shared.lock();
            state.waiting || state.ready.is_some()
        }));
        drawables.shared.lock().waiting
    }
    /// Both of `layer`'s drawables, held, if it gives two.
    fn held_pool(layer: &CAMetalLayer) -> Option<[Drawable; 2]> {
        Some([layer.nextDrawable()?, layer.nextDrawable()?])
    }
    fn counting() -> (crate::PlaneWaker, Arc<AtomicUsize>) {
        let wakes = Arc::new(AtomicUsize::new(0));
        let counted = wakes.clone();
        (
            Arc::new(move || {
                counted.fetch_add(1, Ordering::Relaxed);
            }),
            wakes,
        )
    }
    fn until(mut done: impl FnMut() -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if done() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        false
    }

    #[test]
    fn an_idle_plane_makes_no_thread() {
        let Some(layer) = layer() else { return };
        let drawables = Drawables::new(layer, None);
        assert!(drawables.thread.is_none());
        assert!(drawables.take().is_none());
    }

    #[test]
    fn a_wanted_drawable_is_handed_over_with_one_wake() {
        let Some(layer) = layer() else { return };
        let (wake, wakes) = counting();
        let mut drawables = Drawables::new(layer, Some(wake));
        drawables.want();
        drawables.want();
        assert!(until(|| drawables.ready()));
        assert_eq!(wakes.load(Ordering::Relaxed), 1, "one wake a drawable");
        assert!(drawables.take().is_some());
        assert!(drawables.take().is_none(), "taken once");
        // Not wanted, it waits for nothing more.
        std::thread::sleep(Duration::from_millis(20));
        assert!(!drawables.ready());
        assert_eq!(drawables.handed(), 1);
    }

    #[test]
    fn closing_while_it_waits_never_waits_on_the_main_thread() {
        let Some(layer) = layer() else { return };
        // The pool's two drawables held: the thread waits in nextDrawable.
        // (Where the host gives none, as some virtual machines' GPUs, there is
        // nothing to wait for.)
        let Some(held) = held_pool(&layer) else {
            return;
        };
        let mut drawables = Drawables::new(layer, None);
        drawables.want();
        // Its pool larger than asked for, the thread gets a drawable at once
        // and there is nothing to wait for.
        if !waits(&drawables) {
            return;
        }
        let shared = Arc::downgrade(&drawables.shared);
        let closed = Instant::now();
        drop(drawables);
        assert!(
            closed.elapsed() < Duration::from_millis(50),
            "{:?}",
            closed.elapsed()
        );
        // Its wait ends (a drawable back, or Core Animation's timeout) and
        // the thread with it, dropping what it got.
        drop(held);
        assert!(until(|| shared.strong_count() == 0), "the thread ended");
    }

    #[test]
    fn a_parked_thread_is_joined_when_its_plane_goes() {
        let Some(layer) = layer() else { return };
        let mut drawables = Drawables::new(layer, None);
        drawables.want();
        assert!(until(|| drawables.ready()));
        drop(drawables.take());
        assert!(until(|| !drawables.shared.lock().waiting));
        let shared = Arc::downgrade(&drawables.shared);
        drop(drawables);
        assert_eq!(shared.strong_count(), 0, "joined: nothing left");
    }

    #[test]
    fn a_cancelled_want_parks_the_thread() {
        let Some(layer) = layer() else { return };
        let Some(held) = held_pool(&layer) else {
            return;
        };
        let mut drawables = Drawables::new(layer, None);
        drawables.want();
        // Its pool larger than asked for, the thread gets a drawable at once
        // and there is nothing to wait for.
        if !waits(&drawables) {
            return;
        }
        drawables.cancel();
        drop(held);
        assert!(until(|| !drawables.shared.lock().waiting));
        assert!(!drawables.thread_done(), "parked, not ended");
    }
}
