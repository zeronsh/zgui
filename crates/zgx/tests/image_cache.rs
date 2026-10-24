use std::{
    cell::Cell,
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::Arc,
    task::{Context, Poll, Waker},
};
use zgx::{
    image::ImageData,
    image_cache::{ImageCache, ImageLoadError},
};
fn pixel_image(color: u8) -> Arc<ImageData> {
    Arc::new(ImageData::new(1, 1, vec![color, 0, 0, 255]).unwrap())
}
fn poll<F: Future + Unpin>(future: &mut F) -> Poll<F::Output> {
    Pin::new(future).poll(&mut Context::from_waker(Waker::noop()))
}
#[test]
fn deduplication_bounded_lru_and_failed_retry() {
    let cache = ImageCache::new(4);
    let calls = Rc::new(Cell::new(0));
    let mut first = cache.load("a", {
        let calls = calls.clone();
        move || {
            calls.set(calls.get() + 1);
            async { Ok(pixel_image(1)) }
        }
    });
    let mut second = cache.load("a", || async { panic!("duplicate fetch") });
    let Poll::Ready(Ok(a)) = poll(&mut first) else {
        panic!()
    };
    let Poll::Ready(Ok(b)) = poll(&mut second) else {
        panic!()
    };
    assert!(Arc::ptr_eq(&a, &b));
    assert_eq!(calls.get(), 1);
    assert_eq!(cache.bytes(), 4);
    assert!(poll(&mut cache.load("a", || async { panic!("cache hit") })).is_ready());
    assert!(poll(&mut cache.load("b", || async { Ok(pixel_image(2)) })).is_ready());
    assert_eq!(cache.bytes(), 4);
    let mut failure = cache.load("error", || async { Err(ImageLoadError::from("failed")) });
    assert!(matches!(poll(&mut failure), Poll::Ready(Err(_))));
    assert!(matches!(
        poll(&mut cache.load("error", || async { Ok(pixel_image(3)) })),
        Poll::Ready(Ok(_))
    ));
    cache.clear();
    assert_eq!(cache.bytes(), 0);
}
struct PendingDrop(Rc<Cell<usize>>);
impl Future for PendingDrop {
    type Output = Result<Arc<ImageData>, ImageLoadError>;
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Pending
    }
}
impl Drop for PendingDrop {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}
#[test]
fn final_request_owns_cancellation_and_clear_does_not_poison_new_generation() {
    let cache = ImageCache::default();
    let dropped = Rc::new(Cell::new(0));
    let mut first = cache.load("pending", {
        let dropped = dropped.clone();
        move || PendingDrop(dropped)
    });
    let second = first.clone();
    assert!(poll(&mut first).is_pending());
    drop(first);
    assert_eq!(dropped.get(), 0);
    drop(second);
    assert_eq!(dropped.get(), 1);
    let mut old = cache.load("key", || async { Ok(pixel_image(1)) });
    cache.clear();
    let mut new = cache.load("key", || async { Ok(pixel_image(2)) });
    assert!(poll(&mut old).is_ready());
    assert_eq!(cache.bytes(), 0);
    assert!(poll(&mut new).is_ready());
    assert_eq!(cache.bytes(), 4);
}

#[test]
fn pending_shared_load_wakes_remaining_subscriber_after_peer_cancellation() {
    use std::{
        cell::RefCell,
        sync::atomic::{AtomicUsize, Ordering},
        task::Wake,
    };
    struct Count(AtomicUsize);
    impl Wake for Count {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    struct Gate {
        ready: Rc<Cell<bool>>,
        wake: Rc<RefCell<Option<Waker>>>,
    }
    impl Future for Gate {
        type Output = Result<Arc<ImageData>, ImageLoadError>;
        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            if self.ready.get() {
                Poll::Ready(Ok(pixel_image(9)))
            } else {
                *self.wake.borrow_mut() = Some(cx.waker().clone());
                Poll::Pending
            }
        }
    }
    let cache = ImageCache::default();
    let ready = Rc::new(Cell::new(false));
    let wake = Rc::new(RefCell::new(None));
    let mut first = cache.load("gate", {
        let ready = ready.clone();
        let wake = wake.clone();
        move || Gate { ready, wake }
    });
    let mut second = first.clone();
    let first_count = Arc::new(Count(AtomicUsize::new(0)));
    let second_count = Arc::new(Count(AtomicUsize::new(0)));
    assert!(
        Pin::new(&mut first)
            .poll(&mut Context::from_waker(&Waker::from(first_count.clone())))
            .is_pending()
    );
    assert!(
        Pin::new(&mut second)
            .poll(&mut Context::from_waker(&Waker::from(second_count.clone())))
            .is_pending()
    );
    drop(first);
    ready.set(true);
    wake.borrow_mut().take().unwrap().wake();
    assert_eq!(first_count.0.load(Ordering::SeqCst), 0);
    assert_eq!(second_count.0.load(Ordering::SeqCst), 1);
    assert!(poll(&mut second).is_ready());
}

#[test]
fn async_component_owns_loading_success_error_and_unmount_cancellation() {
    use std::cell::RefCell;
    use zgx::{
        compose::{TaskRunner, prelude::*},
        task::LocalExecutor,
        widgets::Ui,
    };
    let mut ui = Ui::new(120., 80.);
    let executor = Rc::new(RefCell::new(LocalExecutor::new()));
    let dropped = Rc::new(Cell::new(0));
    let pending = ui.mount(provide(
        TaskRunner::from_executor(executor.clone()),
        async_image(
            "pending",
            ImageCache::default(),
            "pending",
            {
                let dropped = dropped.clone();
                move || PendingDrop(dropped)
            },
            text("Loading").id("loading"),
            |e| text(e.to_string()).id("error"),
        ),
    ));
    ui.prepare_frame();
    executor.borrow_mut().tick();
    assert!(pending.find("loading").is_some());
    pending.unmount();
    executor.borrow_mut().tick();
    assert_eq!(dropped.get(), 1);
    let success = ui.mount(provide(
        TaskRunner::from_executor(executor.clone()),
        async_image(
            "loaded",
            ImageCache::default(),
            "ok",
            || async { Ok(pixel_image(42)) },
            text("Loading").id("loading"),
            |e| text(e.to_string()),
        ),
    ));
    executor.borrow_mut().tick();
    ui.prepare_frame();
    assert!(success.find("loading").is_none());
    assert!(
        ui.scene
            .borrow()
            .paint_items()
            .any(|item| matches!(item.kind, zgx::scene::NodeKind::Image(_)))
    );
    success.unmount();
    let error = ui.mount(provide(
        TaskRunner::from_executor(executor.clone()),
        async_image(
            "failed",
            ImageCache::default(),
            "error",
            || async { Err("offline".into()) },
            text("Loading"),
            |e| text(e.to_string()).id("error"),
        ),
    ));
    executor.borrow_mut().tick();
    ui.prepare_frame();
    assert!(error.find("error").is_some());
}

/// Resolves once opened.
#[derive(Clone, Default)]
struct Latch(Rc<std::cell::RefCell<(bool, Option<Waker>)>>);
impl Latch {
    fn open(&self) {
        let waker = {
            let mut state = self.0.borrow_mut();
            state.0 = true;
            state.1.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }
    fn wait(
        self,
        image: Arc<ImageData>,
    ) -> impl Future<Output = Result<Arc<ImageData>, ImageLoadError>> {
        std::future::poll_fn(move |cx| {
            let mut state = self.0.borrow_mut();
            if state.0 {
                Poll::Ready(Ok(image.clone()))
            } else {
                state.1 = Some(cx.waker().clone());
                Poll::Pending
            }
        })
    }
}

#[test]
fn images_with_a_known_shape_reserve_it_before_loading() {
    use std::cell::RefCell;
    use zgx::{
        compose::{TaskRunner, prelude::*},
        scene::Rect,
        task::LocalExecutor,
        widgets::Ui,
    };
    let mut ui = Ui::new(400., 400.);
    let executor = Rc::new(RefCell::new(LocalExecutor::new()));
    let latch = Latch::default();
    let wide = Arc::new(ImageData::new(40, 10, vec![200; 40 * 10 * 4]).unwrap());
    let view = ui.mount(provide(
        TaskRunner::from_executor(executor.clone()),
        column()
            .child(
                async_image(
                    "photo",
                    ImageCache::default(),
                    "photo",
                    {
                        let latch = latch.clone();
                        move || latch.wait(wide)
                    },
                    text("Loading").id("loading"),
                    |e| text(e.to_string()),
                )
                // Width and aspect ratio known before the pixels, as from an
                // API or markdown: the image takes its size at once.
                .w(200.)
                .aspect_ratio(2.)
                .id("photo"),
            )
            .child(text("below").id("below")),
    ));
    let geometry = |ui: &Ui| {
        let scene = ui.scene.borrow();
        (
            scene.bounds(view.find("photo").unwrap()),
            scene.bounds(view.find("below").unwrap()).y,
        )
    };
    ui.prepare_frame();
    executor.borrow_mut().tick();
    ui.prepare_frame();
    assert!(view.find("loading").is_some());
    assert_eq!(geometry(&ui), (Rect::new(0., 0., 200., 100.), 100.));
    latch.open();
    executor.borrow_mut().tick();
    ui.prepare_frame();
    assert!(view.find("loading").is_none());
    // Loaded, a differently shaped image fits the reserved box: nothing moves.
    assert_eq!(geometry(&ui), (Rect::new(0., 0., 200., 100.), 100.));
}
