//! Boxes whose pixels an app draws on the GPU: a 3D view, a game, a
//! decoder's output (`compose::gpu_view`).
//!
//! A view is a painter the renderer asks for a frame, drawn into a texture
//! it composites as an image: clipped, rounded, faded, transformed and
//! damaged as any image is. Each frame is an image of its own identity with
//! no pixels (`ImageData::gpu_frame`), so a renderer draws it only when it
//! is new: a view that is not invalidated costs nothing, and UI changes over
//! it show its last frame again. The core knows nothing of the GPU: the
//! painter is whatever its renderer takes (zgx-gpu's `viewport::Viewport`),
//! found by the view's id (`painter`).
use std::{
    any::Any,
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::{Rc, Weak},
    sync::Arc,
};

use crate::{image::ImageData, reactive::Signal};

/// What a GPU view's frame asks of its renderer: which view draws it, at
/// what size.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct GpuFrame {
    /// The view (`GpuView::id`).
    pub view: u64,
    /// Which of the boxes showing the view it is for: each has a texture of
    /// its own.
    pub shown_by: u64,
    /// Its box's width, in points: the renderer draws it at its own scale.
    pub width: f32,
    /// Its box's height, in points.
    pub height: f32,
}

thread_local! {
    /// Live views' painters by id: renderers look a frame's up as they draw it.
    static VIEWS: RefCell<HashMap<u64, Weak<Inner>>> = RefCell::default();
}

/// The painter of view `view`, if it is alive: what its renderer downcasts
/// to the painter type it takes.
pub fn painter(view: u64) -> Option<Rc<dyn Any>> {
    VIEWS.with(|views| {
        let inner = views.borrow().get(&view)?.upgrade()?;
        Some(inner.painter.clone())
    })
}

/// A box's wish for frames: how many it asked for, and whether it animates.
struct Mounted {
    key: u64,
    requests: Signal<u64>,
    animating: Signal<bool>,
}

struct Inner {
    id: u64,
    painter: Rc<dyn Any>,
    animating: Cell<bool>,
    mounted: RefCell<Vec<Mounted>>,
    next_key: Cell<u64>,
}
impl Drop for Inner {
    fn drop(&mut self) {
        // A thread ending drops its views after its registry: nothing to forget.
        let _ = VIEWS.try_with(|views| views.borrow_mut().remove(&self.id));
    }
}

/// A painter shown by boxes ([`crate::compose::gpu_view`]): it draws a frame
/// when shown for the first time, when resized, when invalidated and, while
/// it animates, once per display refresh. Clones share it.
#[derive(Clone)]
pub struct GpuView(Rc<Inner>);
impl GpuView {
    /// A view drawn by `painter`, which its renderer must take (zgx-gpu's
    /// `viewport::view` makes one for it).
    pub fn new(painter: impl Any) -> Self {
        let id = crate::image::next_identity().expect("GPU view identity exhausted");
        let inner = Rc::new(Inner {
            id,
            painter: Rc::new(painter),
            animating: Cell::new(false),
            mounted: RefCell::default(),
            next_key: Cell::new(0),
        });
        VIEWS.with(|views| views.borrow_mut().insert(id, Rc::downgrade(&inner)));
        Self(inner)
    }
    /// Stable for the view's lifetime: its frames name it (`GpuFrame::view`).
    pub fn id(&self) -> u64 {
        self.0.id
    }
    /// Its painter, as [`GpuView::new`] took it.
    pub fn painter(&self) -> &Rc<dyn Any> {
        &self.0.painter
    }
    /// Draw a new frame for every box showing it, before they are next
    /// drawn. Calls between two draws make one frame.
    pub fn invalidate(&self) {
        for mounted in self.0.mounted.borrow().iter() {
            mounted.requests.mutate(|count| {
                *count = count.wrapping_add(1);
                true
            });
        }
    }
    /// Whether it draws a frame every display refresh while a box shows it
    /// (a spinning model, a game). Off, it draws only when invalidated or
    /// resized, and an idle window does no work.
    pub fn animate(&self, animating: bool) {
        self.0.animating.set(animating);
        for mounted in self.0.mounted.borrow().iter() {
            mounted.animating.set(animating);
        }
    }
    /// Whether it animates (`animate`).
    pub fn animating(&self) -> bool {
        self.0.animating.get()
    }
    /// A box's signals, until the returned guard drops.
    pub(crate) fn mount(&self, requests: Signal<u64>, animating: Signal<bool>) -> MountGuard {
        let key = self.0.next_key.get();
        self.0.next_key.set(key.wrapping_add(1));
        animating.set(self.0.animating.get());
        self.0.mounted.borrow_mut().push(Mounted {
            key,
            requests,
            animating,
        });
        MountGuard {
            view: Rc::downgrade(&self.0),
            key,
        }
    }
}
impl std::fmt::Debug for GpuView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GpuView")
            .field("id", &self.0.id)
            .field("animating", &self.0.animating.get())
            .finish()
    }
}

/// Forgets a box's signals when the box goes.
pub(crate) struct MountGuard {
    view: Weak<Inner>,
    key: u64,
}
impl MountGuard {
    /// The box, among those showing the view (`GpuFrame::shown_by`).
    pub(crate) fn key(&self) -> u64 {
        self.key
    }
}
impl Drop for MountGuard {
    fn drop(&mut self) {
        if let Some(view) = self.view.upgrade() {
            view.mounted.borrow_mut().retain(|m| m.key != self.key);
        }
    }
}

/// A box's frames: two images, one shown and one spare, each made the next
/// frame in turn in place (a new identity, its size), so a view drawing every
/// refresh allocates nothing.
#[derive(Default)]
pub(crate) struct FrameRing {
    frames: [Option<Arc<ImageData>>; 2],
    shown: usize,
}
impl FrameRing {
    /// The next frame: the spare made `frame` if no scene still holds it,
    /// else a new image.
    pub(crate) fn next(&mut self, frame: GpuFrame) -> Arc<ImageData> {
        let spare = 1 - self.shown;
        let reused = self.frames[spare]
            .as_mut()
            .and_then(Arc::get_mut)
            .is_some_and(|image| image.restamp(frame).is_ok());
        if !reused {
            self.frames[spare] = Some(Arc::new(
                ImageData::gpu(frame).expect("image identity exhausted"),
            ));
        }
        self.shown = spare;
        self.frames[spare].clone().unwrap()
    }
}
