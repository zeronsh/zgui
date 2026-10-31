//! Bounded, deduplicated async image loading with application-supplied transport.
//! Pending work is owned by its request futures; dropping the final request cancels it.
use crate::image::ImageData;
use std::{
    cell::RefCell,
    collections::HashMap,
    future::Future,
    pin::Pin,
    rc::{Rc, Weak},
    sync::{Arc, Mutex},
    task::{Context, Poll, Wake, Waker},
};
/// Why an image could not be loaded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageLoadError(pub Arc<str>);
impl From<&str> for ImageLoadError {
    fn from(value: &str) -> Self {
        Self(value.into())
    }
}
impl From<String> for ImageLoadError {
    fn from(value: String) -> Self {
        Self(value.into())
    }
}
impl std::fmt::Display for ImageLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl std::error::Error for ImageLoadError {}
type ResultImage = Result<Arc<ImageData>, ImageLoadError>;
type LoadFuture = Pin<Box<dyn Future<Output = ResultImage>>>;
enum Entry {
    Pending(Weak<Shared>),
    Ready { image: Arc<ImageData>, used: u64 },
}
struct Cache {
    entries: HashMap<String, Entry>,
    bytes: usize,
    budget: usize,
    clock: u64,
}
/// Ready pixels use a bounded LRU; pending keys and UTF-8 key lengths are bounded
/// separately. Errors are not cached, so a new request can retry.
#[derive(Clone)]
pub struct ImageCache(Rc<RefCell<Cache>>);
impl Default for ImageCache {
    fn default() -> Self {
        Self::new(16 * 1024 * 1024)
    }
}
impl ImageCache {
    /// A cache keeping at most `byte_budget` bytes of loaded pixels, capped at 64
    /// MiB (the default is 16 MiB).
    pub fn new(byte_budget: usize) -> Self {
        Self(Rc::new(RefCell::new(Cache {
            entries: HashMap::new(),
            bytes: 0,
            budget: byte_budget.min(64 * 1024 * 1024),
            clock: 0,
        })))
    }
    /// The bytes of loaded pixels it keeps.
    pub fn bytes(&self) -> usize {
        self.0.borrow().bytes
    }
    /// Forget every image and pending load. Loads in flight still finish for their
    /// requests, but are not cached.
    pub fn clear(&self) {
        let mut cache = self.0.borrow_mut();
        cache.entries.clear();
        cache.bytes = 0;
    }
    /// The image for `key`: the cached one, a load in flight for the same key, or
    /// else the future `fetch` makes. Fails for keys over 1024 bytes and when 256
    /// loads are pending. An image larger than the budget is not cached.
    pub fn load<F: Future<Output = ResultImage> + 'static>(
        &self,
        key: impl Into<String>,
        fetch: impl FnOnce() -> F,
    ) -> ImageRequest {
        let key = key.into();
        if key.len() > 1024 {
            return ImageRequest::ready(Err("image key exceeds 1024 bytes".into()));
        }
        let mut cache = self.0.borrow_mut();
        cache.clock = cache.clock.wrapping_add(1);
        let clock = cache.clock;
        cache
            .entries
            .retain(|_, entry| !matches!(entry,Entry::Pending(weak) if weak.strong_count()==0));
        match cache.entries.get_mut(&key) {
            Some(Entry::Ready { image, used }) => {
                *used = clock;
                return ImageRequest::ready(Ok(image.clone()));
            }
            Some(Entry::Pending(weak)) => {
                if let Some(shared) = weak.upgrade() {
                    return ImageRequest::new(shared);
                }
            }
            None => {}
        }
        while cache.entries.len() >= 256 {
            if !evict(&mut cache) {
                return ImageRequest::ready(Err(
                    "image cache pending request capacity exhausted".into()
                ));
            }
        }
        let shared = Rc::new(Shared {
            future: RefCell::new(None),
            result: RefCell::new(None),
            wake: Arc::new(WakeAll::default()),
            next: std::cell::Cell::new(0),
            cache: Rc::downgrade(&self.0),
            key: key.clone(),
        });
        cache
            .entries
            .insert(key, Entry::Pending(Rc::downgrade(&shared)));
        drop(cache);
        *shared.future.borrow_mut() = Some(Box::pin(fetch()));
        ImageRequest::new(shared)
    }
}
fn evict(cache: &mut Cache) -> bool {
    let key = cache
        .entries
        .iter()
        .filter_map(|(key, e)| match e {
            Entry::Ready { used, .. } => Some((key, *used)),
            _ => None,
        })
        .min_by_key(|(_, used)| *used)
        .map(|(key, _)| key.clone());
    let Some(key) = key else {
        return false;
    };
    if let Some(Entry::Ready { image, .. }) = cache.entries.remove(&key) {
        cache.bytes -= image.bytes();
    }
    true
}
#[derive(Default)]
struct WakeAll(Mutex<HashMap<u64, Waker>>);
impl Wake for WakeAll {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        let waiters: Vec<_> = self.0.lock().unwrap().values().cloned().collect();
        for waiter in waiters {
            waiter.wake();
        }
    }
}
struct Shared {
    future: RefCell<Option<LoadFuture>>,
    result: RefCell<Option<ResultImage>>,
    wake: Arc<WakeAll>,
    next: std::cell::Cell<u64>,
    cache: Weak<RefCell<Cache>>,
    key: String,
}
/// A future of one [`ImageCache::load`]'s result. Clones share the load;
/// dropping the last one cancels it.
pub struct ImageRequest {
    shared: Rc<Shared>,
    id: u64,
}
impl ImageRequest {
    fn new(shared: Rc<Shared>) -> Self {
        let id = shared.next.get();
        shared
            .next
            .set(id.checked_add(1).expect("image request identity exhausted"));
        Self { shared, id }
    }
    fn ready(result: ResultImage) -> Self {
        Self::new(Rc::new(Shared {
            future: RefCell::new(None),
            result: RefCell::new(Some(result)),
            wake: Arc::new(WakeAll::default()),
            next: std::cell::Cell::new(0),
            cache: Weak::new(),
            key: String::new(),
        }))
    }
}
impl Clone for ImageRequest {
    fn clone(&self) -> Self {
        Self::new(self.shared.clone())
    }
}
impl Drop for ImageRequest {
    fn drop(&mut self) {
        self.shared.wake.0.lock().unwrap().remove(&self.id);
    }
}
impl Future for ImageRequest {
    type Output = ResultImage;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Some(result) = self.shared.result.borrow().clone() {
            return Poll::Ready(result);
        }
        self.shared
            .wake
            .0
            .lock()
            .unwrap()
            .insert(self.id, cx.waker().clone());
        let Ok(mut future) = self.shared.future.try_borrow_mut() else {
            return Poll::Pending;
        };
        let Some(load) = future.as_mut() else {
            return Poll::Pending;
        };
        let waker = Waker::from(self.shared.wake.clone());
        let result = match load.as_mut().poll(&mut Context::from_waker(&waker)) {
            Poll::Ready(result) => result,
            Poll::Pending => return Poll::Pending,
        };
        let completed = future.take();
        drop(future);
        drop(completed);
        *self.shared.result.borrow_mut() = Some(result.clone());
        if let Some(cache) = self.shared.cache.upgrade() {
            let mut cache = cache.borrow_mut();
            // clear/reload may have replaced this generation while it was in flight.
            let current = matches!(cache.entries.get(&self.shared.key),Some(Entry::Pending(weak)) if weak.ptr_eq(&Rc::downgrade(&self.shared)));
            if current {
                cache.entries.remove(&self.shared.key);
                if let Ok(image) = &result {
                    let bytes = image.bytes();
                    if bytes <= cache.budget {
                        while cache.bytes + bytes > cache.budget {
                            if !evict(&mut cache) {
                                break;
                            }
                        }
                        cache.bytes += bytes;
                        let used = cache.clock;
                        cache.entries.insert(
                            self.shared.key.clone(),
                            Entry::Ready {
                                image: image.clone(),
                                used,
                            },
                        );
                    }
                }
            }
        }
        self.shared.wake.wake_by_ref();
        Poll::Ready(result)
    }
}
