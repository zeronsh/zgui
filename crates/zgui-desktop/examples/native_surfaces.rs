//! macOS CoreVideo frames, imported directly into Metal without CPU readback.
#[cfg(target_os = "macos")]
mod mac {
    use objc2_core_foundation::{CFBoolean, CFDictionary, CFRetained, CFString, CFType};
    use objc2_core_video::*;
    use std::{ptr::NonNull, rc::Rc};
    use zgui::{compose::prelude::*, native_surface::NativeSurface};
    use zgui_desktop::{Application, WindowOptions};
    fn frame(nv12: bool) -> Rc<NativeSurface> {
        let empty = CFDictionary::<CFType, CFType>::empty();
        // SAFETY: reads two CoreVideo constants, immutable CFStrings it exports.
        let attributes = CFDictionary::<CFString, CFType>::from_slices(
            &[unsafe { kCVPixelBufferIOSurfacePropertiesKey }, unsafe {
                kCVPixelBufferMetalCompatibilityKey
            }],
            &[empty.as_ref(), CFBoolean::new(true).as_ref()],
        );
        let mut pointer = std::ptr::null_mut();
        // SAFETY: attribute keys and values of the types CoreVideo documents (a
        // dictionary, a boolean), and an out-pointer to a local it writes.
        assert_eq!(
            unsafe {
                CVPixelBufferCreate(
                    None,
                    4,
                    4,
                    if nv12 {
                        kCVPixelFormatType_420YpCbCr8BiPlanarFullRange
                    } else {
                        kCVPixelFormatType_32BGRA
                    },
                    Some(attributes.as_opaque()),
                    NonNull::from(&mut pointer),
                )
            },
            0
        );
        // SAFETY: a successful Create returns a non-null +1 reference, which
        // `CFRetained` takes over.
        let buffer = unsafe { CFRetained::from_raw(NonNull::new(pointer).unwrap()) };
        // SAFETY: the base address is used only while locked: each NV12 plane's
        // `rows * stride` bytes, or the 4x4 BGRA rows `stride` bytes apart, are
        // within the buffer. Unlocked, its pixels are written (CPU writes,
        // finished) and nothing changes them again, premultiplied (opaque) BGRA
        // or full-range NV12, IOSurface-backed and Metal-compatible as
        // `from_pixel_buffer` requires.
        unsafe {
            assert_eq!(
                CVPixelBufferLockBaseAddress(&buffer, CVPixelBufferLockFlags::empty()),
                0
            );
            if nv12 {
                for plane in 0..2 {
                    let rows = CVPixelBufferGetHeightOfPlane(&buffer, plane);
                    let stride = CVPixelBufferGetBytesPerRowOfPlane(&buffer, plane);
                    std::ptr::write_bytes(
                        CVPixelBufferGetBaseAddressOfPlane(&buffer, plane).cast::<u8>(),
                        128,
                        rows * stride,
                    );
                }
            } else {
                let pointer = CVPixelBufferGetBaseAddress(&buffer).cast::<u8>();
                let stride = CVPixelBufferGetBytesPerRow(&buffer);
                for y in 0..4 {
                    for x in 0..4 {
                        std::ptr::copy_nonoverlapping(
                            [20, 40, 220, 255].as_ptr(),
                            pointer.add(y * stride + x * 4),
                            4,
                        );
                    }
                }
            }
            assert_eq!(
                CVPixelBufferUnlockBaseAddress(&buffer, CVPixelBufferLockFlags::empty()),
                0
            );
            Rc::new(NativeSurface::from_pixel_buffer(buffer).unwrap())
        }
    }
    pub fn run() -> Result<(), Box<dyn std::error::Error>> {
        let smoke = std::env::args().any(|arg| arg == "--smoke-test");
        Application::new()
            .window(WindowOptions {
                title: "zgui native CoreVideo surface".into(),
                width: 360.,
                height: 280.,
                ..Default::default()
            })
            .run(move |cx| {
                let source = cx.ui.signal(frame(false));
                let read = source.clone();
                cx.render(
                    column()
                        .p(24.)
                        .gap(16.)
                        .child("Native CoreVideo → Metal")
                        .child(
                            native_surface_signal("Video frame", move || read.get())
                                .w(280.)
                                .h(180.)
                                .p(8.)
                                .object_fit(ObjectFit::Contain)
                                .bg(rgb(0x243447)),
                        ),
                );
                if smoke {
                    let window = cx.window.clone();
                    cx.tasks.spawn(async move {
                        for index in 1..=4 {
                            zgui::timer::sleep(std::time::Duration::from_millis(300)).await;
                            source.set(frame(index % 2 == 0));
                            println!("NATIVE_SURFACE frame={} nv12={}", index, index % 2 == 0);
                        }
                        zgui::timer::sleep(std::time::Duration::from_millis(300)).await;
                        window.close();
                    });
                }
            })
    }
}
#[cfg(target_os = "macos")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    mac::run()
}
#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("native_surfaces requires macOS and Metal");
}
