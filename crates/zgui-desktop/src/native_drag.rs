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
/// (top-down from the primary screen's top, in points; on Windows, the
/// screen's pixels, as `content_origin` gives: `popup_window::global_scale`).
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
    let data = x11::offers(items);
    !data.is_empty()
        && crate::wayland_data::start_drag(window.clone(), data, image, point, Box::new(send))
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
                SOURCE.with(|source| source.borrow_mut().take());
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
        // SAFETY: a bitmap allocating its own planes (null), 8-bit RGBA,
        // premultiplied, rows of `width * 4` bytes; filled below.
        let rep: Retained<NSBitmapImageRep> = unsafe {
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
        let data = rep.bitmapData();
        if data.is_null() {
            return None;
        }
        // SAFETY: the bitmap holds `width * height * 4` bytes.
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
        // SAFETY: winit's content view, alive while its window is.
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
                // SAFETY: the contents are an NSImage (or none).
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
        // SAFETY: NSObject's initializer.
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
        SOURCE.with(|current| *current.borrow_mut() = Some(source));
        true
    }
}
