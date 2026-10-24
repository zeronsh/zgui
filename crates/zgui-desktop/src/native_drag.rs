//! Drags handed to the platform as they leave the app's windows, so other
//! apps take what they carry (`View::drag_export`): Finder files, Mail text
//! and links, a browser images.
//!
//! Inside its window a drag is zgui's own (typed, its preview a popup
//! window). As it leaves the window, the host begins an `NSDraggingSession`
//! with its items and the preview's last frame as the drag image (drawn
//! once; the window server moves it). The session's moves and end come back
//! to the host (`NativeDrag`) as the window's pointer: back over the window,
//! its destinations hear it as before, and a release there drops on them
//! (AppKit's own drop is refused inside the app). Another app taking it is
//! the drag's acceptance; nothing taking it, the preview slides back.
//!
//! Windows hands it to `DoDragDrop` (`native_drag/win32.rs`): Shell's data
//! object with the items in Windows' formats, the preview's last frame as
//! the drag image (`IDragSourceHelper`; Windows draws and moves it), and a
//! drop source reporting the pointer. A release over one of the app's own
//! windows is cancelled for Windows and dropped by zgui, as AppKit's own
//! drop is refused there. Its modal loop runs in place of winit's, so the
//! host steps and paints its windows after each event meanwhile.
//!
//! X11 speaks XDND (`native_drag/x11.rs`): the pointer stays the window's
//! (the button's implicit grab), so the manager hands it its moves, and the
//! preview stays the window of its own it was.
//!
//! Wayland hands it to the compositor (`wl_data_device.start_drag`, by the
//! data worker on winit's display, from the press that began it), the
//! preview drawn once as its icon. The compositor has the pointer then: back
//! over the window it came from, the worker hears it as a drop target, and
//! refuses it, so a release there drops on the window's destinations.
use zgui::input::DragItem;

/// The platform's drag, as the host hears it: in global coordinates
/// (top-down from the primary screen's top, in points; on Windows, screen
/// pixels at the dragging window's scale, as `content_origin` gives).
#[derive(Clone, Copy, Debug)]
pub(crate) enum NativeDrag {
    Moved {
        x: f64,
        y: f64,
    },
    /// `accepted`: another app took it.
    Ended {
        x: f64,
        y: f64,
        accepted: bool,
    },
    /// Over another of the app's windows, at `x`, `y` in its coordinates
    /// (Wayland, where windows have no place on screen): `window`, its
    /// host's token.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    MovedIn {
        window: u64,
        x: f64,
        y: f64,
    },
    /// Released over another of the app's windows (Wayland): it drops there.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    DroppedIn {
        window: u64,
        x: f64,
        y: f64,
    },
    /// Released over none of the app's windows and taken by no app
    /// (Wayland): a drag of the app's own, carrying nothing to other apps,
    /// is torn off, as one released past every window; one carrying an
    /// export ends untaken (`Ended`).
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Left {
        x: f64,
        y: f64,
    },
}

/// A window's drag handed to the platform, as its host keeps it: whether the
/// platform has it, and whether the window lost focus meanwhile.
///
/// Only the platform's end ends the hand-off, whether or not zgui's drag
/// is still under way then: the drag may have ended on zgui's side first
/// (its source went), and a hand-off kept past it would keep every later
/// drag in its window. Losing focus does not end it either (an app taking
/// the drop may come to the front first): the window deactivates once the
/// platform is done, so the source hears whether it was taken.
#[derive(Debug, Default)]
pub(crate) struct Handoff {
    active: bool,
    deactivated: bool,
}
impl Handoff {
    /// Whether the platform has the drag.
    pub(crate) fn active(&self) -> bool {
        self.active
    }
    pub(crate) fn begun(&mut self) {
        self.active = true;
    }
    /// The window lost focus: whether it deactivates now (not while the
    /// platform has its drag).
    pub(crate) fn deactivate(&mut self) -> bool {
        self.deactivated = self.active;
        !self.active
    }
    /// The window has focus again: nothing held back is due.
    pub(crate) fn activate(&mut self) {
        self.deactivated = false;
    }
    /// The platform's drag ended: whether the window's deactivation, held
    /// back meanwhile, is due now.
    pub(crate) fn ended(&mut self) -> bool {
        self.active = false;
        std::mem::take(&mut self.deactivated)
    }
}

/// The drag image: premultiplied BGRA pixels at `scale`, and the pointer
/// within it, in points.
#[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
pub(crate) struct DragImage {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
    pub scale: f32,
    pub grab: (f32, f32),
}

#[cfg(windows)]
mod win32;
#[cfg(windows)]
pub(crate) use win32::begin;

#[cfg(target_os = "linux")]
mod x11;
#[cfg(target_os = "linux")]
pub(crate) use x11::{active, cancel, end_session, moved, released};

/// The type a drag of the app's own is offered as on Wayland: no other app
/// takes it.
#[cfg(target_os = "linux")]
pub(crate) const INTERNAL: &str = "application/x-zgui-drag";

/// Hand the drag under way in `window` to the platform, carrying `items`:
/// on X11 an XDND source of zgui's (`x11`), on Wayland the compositor's drag
/// (`wl_data_device.start_drag`, by the data worker) with `image` as its
/// icon; `point` is where the pointer left, in the window's points.
#[cfg(target_os = "linux")]
pub(crate) fn begin(
    window: &std::sync::Arc<winit::window::Window>,
    items: &[DragItem],
    image: Option<DragImage>,
    point: (f64, f64),
    send: impl Fn(NativeDrag) + Send + 'static,
) -> bool {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let wayland = window
        .window_handle()
        .is_ok_and(|handle| matches!(handle.as_raw(), RawWindowHandle::Wayland(_)));
    if !wayland {
        return x11::begin(window, items, image, send);
    }
    // A drag carrying nothing to other apps (a tab) is the compositor's too,
    // so the app's other windows hear it: offered as a type of zgui's own,
    // which no other app takes.
    let mut data = x11::offers(items);
    if data.is_empty() {
        data.push((INTERNAL, Vec::new()));
    }
    crate::wayland_data::start_drag(window.clone(), data, image, point, Box::new(send))
}

#[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
pub(crate) fn begin(
    _: &winit::window::Window,
    _: &[DragItem],
    _: Option<DragImage>,
    _: impl Fn(NativeDrag) + 'static,
) -> bool {
    false
}

#[cfg(target_os = "macos")]
pub(crate) use macos::begin;

#[cfg(target_os = "macos")]
mod macos {
    use super::{DragImage, DragItem, NativeDrag};
    use objc2::{
        AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
        rc::Retained,
        runtime::{NSObject, ProtocolObject},
    };
    use objc2_app_kit::{
        NSApplication, NSBitmapImageRep, NSDragOperation, NSDraggingContext, NSDraggingItem,
        NSDraggingSession, NSDraggingSource, NSEventType, NSImage, NSPasteboardWriting, NSScreen,
        NSView,
    };
    use objc2_foundation::{
        NSArray, NSData, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString, NSURL,
    };
    use std::cell::RefCell;

    pub(crate) struct SourceState {
        send: Box<dyn Fn(NativeDrag)>,
    }
    // SAFETY: an NSObject subclass, main-thread only (AppKit runs drags
    // there), with nothing to clean up beyond its ivars; its methods have
    // `NSDraggingSource`'s signatures (NSDragOperation and
    // NSDraggingContext are NSUInteger and NSInteger, as objc2 declares).
    define_class!(
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "ZguiDragSource"]
        #[ivars = SourceState]
        pub(crate) struct DragSource;
        unsafe impl NSObjectProtocol for DragSource {}
        unsafe impl NSDraggingSource for DragSource {
            #[unsafe(method(draggingSession:sourceOperationMaskForDraggingContext:))]
            fn operations(
                &self,
                _session: &NSDraggingSession,
                context: NSDraggingContext,
            ) -> NSDragOperation {
                // Inside the app, zgui's destinations take it (the host
                // drops on them), not AppKit's (winit's file drops).
                if context == NSDraggingContext::WithinApplication {
                    return NSDragOperation::None;
                }
                // Copied (never moved: Finder would delete the file), or a
                // link (a .webloc), as the destination prefers.
                NSDragOperation::Copy | NSDragOperation::Link | NSDragOperation::Generic
            }
            #[unsafe(method(draggingSession:movedToPoint:))]
            fn moved(&self, _session: &NSDraggingSession, point: NSPoint) {
                let (x, y) = top_down(point);
                (self.ivars().send)(NativeDrag::Moved { x, y });
            }
            #[unsafe(method(draggingSession:endedAtPoint:operation:))]
            fn ended(
                &self,
                _session: &NSDraggingSession,
                point: NSPoint,
                operation: NSDragOperation,
            ) {
                let (x, y) = top_down(point);
                (self.ivars().send)(NativeDrag::Ended {
                    x,
                    y,
                    accepted: operation != NSDragOperation::None,
                });
                // Released once the event is done, not now: `SOURCE` may
                // hold the last reference to `self`, which is borrowed until
                // this method returns.
                if let Some(source) = SOURCE.with(|source| source.borrow_mut().take()) {
                    let _ = Retained::autorelease_ptr(source);
                }
            }
        }
    );
    thread_local! {
        /// The session's source, alive while it runs.
        static SOURCE: RefCell<Option<Retained<DragSource>>> = const { RefCell::new(None) };
    }
    fn top_down(point: NSPoint) -> (f64, f64) {
        let height = MainThreadMarker::new()
            .and_then(|mtm| NSScreen::screens(mtm).firstObject())
            .map_or(0., |screen| screen.frame().size.height);
        (point.x, height - point.y)
    }

    /// What one item writes to the drag pasteboard.
    fn writer(item: &DragItem) -> Option<Retained<ProtocolObject<dyn NSPasteboardWriting>>> {
        Some(match item {
            DragItem::Text(text) => ProtocolObject::from_retained(NSString::from_str(text)),
            DragItem::Url(url) => {
                ProtocolObject::from_retained(NSURL::URLWithString(&NSString::from_str(url))?)
            }
            DragItem::File(path) => ProtocolObject::from_retained(NSURL::fileURLWithPath(
                &NSString::from_str(&path.to_string_lossy()),
            )),
            DragItem::Image(bytes) => {
                let data = NSData::with_bytes(bytes);
                ProtocolObject::from_retained(NSImage::initWithData(NSImage::alloc(), &data)?)
            }
        })
    }

    /// `image`'s pixels as an image of its size in points.
    fn ns_image(image: &DragImage) -> Option<Retained<NSImage>> {
        let (width, height) = (image.width as isize, image.height as isize);
        // The bitmap holds exactly these pixels: no more are copied into it.
        if image.bgra.len() != image.width as usize * image.height as usize * 4 {
            return None;
        }
        // SAFETY: `initWithBitmapDataPlanes:…` with its declared types (a
        // null planes pointer: the bitmap allocates its own), 8-bit RGBA,
        // premultiplied, rows of `width * 4` bytes; nil (a size it refuses)
        // is checked. Filled below.
        let rep: Option<Retained<NSBitmapImageRep>> = unsafe {
            let planes: *mut *mut u8 = std::ptr::null_mut();
            msg_send![
                NSBitmapImageRep::alloc(),
                initWithBitmapDataPlanes: planes,
                pixelsWide: width,
                pixelsHigh: height,
                bitsPerSample: 8isize,
                samplesPerPixel: 4isize,
                hasAlpha: true,
                isPlanar: false,
                colorSpaceName: &*NSString::from_str("NSDeviceRGBColorSpace"),
                bytesPerRow: width * 4,
                bitsPerPixel: 32isize
            ]
        };
        let rep = rep?;
        let data = rep.bitmapData();
        if data.is_null() || rep.bytesPerRow() != width * 4 {
            return None;
        }
        // SAFETY: the bitmap's own buffer of `height` rows of `width * 4`
        // bytes, which is `bgra`'s length (checked above); nothing else
        // reads or writes it until `rep` is handed on below.
        let pixels = unsafe { std::slice::from_raw_parts_mut(data, image.bgra.len()) };
        let (pixels, _) = pixels.as_chunks_mut::<4>();
        let (bgra, _) = image.bgra.as_chunks::<4>();
        for (out, [b, g, r, a]) in pixels.iter_mut().zip(bgra) {
            *out = [*r, *g, *b, *a];
        }
        let size = NSSize::new(
            image.width as f64 / image.scale as f64,
            image.height as f64 / image.scale as f64,
        );
        let ns_image = NSImage::initWithSize(NSImage::alloc(), size);
        ns_image.addRepresentation(&rep);
        Some(ns_image)
    }

    /// Hand the drag under way in `window` (a mouse drag AppKit is
    /// delivering) to AppKit, carrying `items`. False when it cannot begin
    /// (nothing to carry, or no mouse drag to begin it from).
    pub(crate) fn begin(
        window: &winit::window::Window,
        items: &[DragItem],
        image: Option<DragImage>,
        send: impl Fn(NativeDrag) + 'static,
    ) -> bool {
        use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let Some(mtm) = MainThreadMarker::new() else {
            return false;
        };
        let Ok(handle) = window.window_handle() else {
            return false;
        };
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return false;
        };
        // SAFETY: winit's content view, alive while its window is (borrowed
        // here); this is the main thread (`mtm`).
        let Some(view) = (unsafe { Retained::<NSView>::retain(handle.ns_view.as_ptr().cast()) })
        else {
            return false;
        };
        let Some(event) = NSApplication::sharedApplication(mtm).currentEvent() else {
            return false;
        };
        if !matches!(
            event.r#type(),
            NSEventType::LeftMouseDragged | NSEventType::LeftMouseDown
        ) {
            return false;
        }
        let point = view.convertPoint_fromView(event.locationInWindow(), None);
        let contents = image.as_ref().and_then(ns_image);
        let size = contents
            .as_ref()
            .map_or(NSSize::new(1., 1.), |image| image.size());
        let (grab_x, grab_y) = image.as_ref().map_or((0., 0.), |image| image.grab);
        // Under the pointer where the preview was.
        let frame = NSRect::new(
            NSPoint::new(
                point.x - grab_x as f64,
                if view.isFlipped() {
                    point.y - grab_y as f64
                } else {
                    point.y - (size.height - grab_y as f64)
                },
            ),
            size,
        );
        let dragging: Vec<Retained<NSDraggingItem>> = items
            .iter()
            .filter_map(writer)
            .enumerate()
            .map(|(index, writer)| {
                let item =
                    NSDraggingItem::initWithPasteboardWriter(NSDraggingItem::alloc(), &writer);
                // The image once, on the first item.
                let contents = contents.as_deref().filter(|_| index == 0);
                // SAFETY: `contents` is `id` in Objective-C: an NSImage (or
                // none) is one the item can show.
                unsafe {
                    item.setDraggingFrame_contents(frame, contents.map(|image| &**image as _))
                };
                item
            })
            .collect();
        if dragging.is_empty() {
            return false;
        }
        let source = DragSource::alloc(mtm).set_ivars(SourceState {
            send: Box::new(send),
        });
        // SAFETY: NSObject's `init`, on a fresh allocation with its ivars set.
        let source: Retained<DragSource> = unsafe { msg_send![super(source), init] };
        let session = view.beginDraggingSessionWithItems_event_source(
            &NSArray::from_retained_slice(&dragging),
            &event,
            ProtocolObject::from_ref(&*source),
        );
        // A drag nothing takes slides back to its source: the host slides
        // the preview there, rather than AppKit to where the session began
        // (the window's edge).
        session.setAnimatesToStartingPositionsOnCancelOrFail(false);
        // Kept until the session ends: whether AppKit retains its source is
        // undocumented. A window hands off no drag while one is under way
        // (`Handoff`), and AppKit runs one at a time, so none is replaced.
        SOURCE.with(|current| *current.borrow_mut() = Some(source));
        true
    }

    #[cfg(test)]
    mod tests {
        use super::{DragImage, ns_image};

        /// A drag image's pixels are copied into a bitmap of its size: more
        /// (or fewer) than its size holds make no image.
        #[test]
        fn a_drag_image_whose_pixels_miss_its_size_makes_no_image() {
            let image = |bgra: Vec<u8>| DragImage {
                width: 2,
                height: 3,
                bgra,
                scale: 1.,
                grab: (0., 0.),
            };
            assert!(ns_image(&image(vec![255; 2 * 3 * 4])).is_some());
            assert!(ns_image(&image(vec![255; 2 * 3 * 4 + 4])).is_none());
            assert!(ns_image(&image(vec![255; 4])).is_none());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Handoff;

    #[test]
    fn a_handoff_ends_with_the_platforms_drag() {
        let mut handoff = Handoff::default();
        handoff.begun();
        assert!(handoff.active());
        assert!(!handoff.ended());
        assert!(!handoff.active());
        // The next drag hands off again.
        handoff.begun();
        assert!(handoff.active());
    }

    #[test]
    fn losing_focus_waits_for_the_platforms_drag_to_end() {
        let mut handoff = Handoff::default();
        assert!(handoff.deactivate());
        handoff.activate();
        handoff.begun();
        // The app taking the drop came to the front.
        assert!(!handoff.deactivate());
        assert!(handoff.ended());
        assert!(!handoff.active());
        // Held back once, not again.
        assert!(!handoff.ended());
        assert!(handoff.deactivate());
    }

    #[test]
    fn focus_back_during_the_platforms_drag_cancels_the_held_deactivation() {
        let mut handoff = Handoff::default();
        handoff.begun();
        assert!(!handoff.deactivate());
        handoff.activate();
        assert!(!handoff.ended());
        assert!(!handoff.active());
    }
}
