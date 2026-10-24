#![cfg(target_os = "macos")]
use objc2_core_foundation::{CFBoolean, CFDictionary, CFRetained, CFString, CFType};
use objc2_core_video::*;
use std::{ptr::NonNull, rc::Rc};
use zgx::{
    native_surface::NativeSurface,
    scene::{NodeKind, Rect, Scene, Style},
};
use zgx_gpu::GpuRenderer;
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
#[test]
fn metal_imports_native_bgra_and_nv12_reuses_frames_and_releases_on_teardown() {
    for nv12 in [false, true] {
        let frame = frame(nv12);
        let weak = Rc::downgrade(&frame);
        let mut scene = Scene::new(32., 32.);
        let node = scene.append(
            scene.root(),
            NodeKind::NativeSurface(frame),
            Style {
                width: Some(32.),
                height: Some(32.),
                ..Default::default()
            },
        );
        let mut gpu = GpuRenderer::new(32, 32).unwrap();
        let damage = scene.flush().damage;
        let stats = gpu.render(&scene, &damage).unwrap();
        assert_eq!(stats.image_uploads, 0);
        assert_eq!(stats.native_surface_imports, 1);
        assert_eq!(stats.native_surface_conversions, usize::from(nv12));
        let pixels = gpu.readback().unwrap();
        let pixel = &pixels[(16 * 32 + 16) * 4..(16 * 32 + 16) * 4 + 4];
        if nv12 {
            assert!(
                pixel[..3].iter().all(|c| (124..=132).contains(c)),
                "{pixel:?}"
            );
        } else {
            assert_eq!(pixel, &[220, 40, 20, 255]);
        }
        let stats = gpu.render(&scene, &[Rect::new(0., 0., 32., 32.)]).unwrap();
        assert_eq!(stats.native_surface_imports, 0);
        assert_eq!(stats.native_surface_conversions, 0);
        scene.remove(node);
        let damage = scene.flush().damage;
        gpu.render(&scene, &damage).unwrap();
        drop(gpu);
        assert!(
            weak.upgrade().is_none(),
            "window teardown retained native frame"
        );
    }
}

#[test]
fn native_surface_component_retains_nodes_fits_padding_and_releases_bindings() {
    use zgx::{compose::prelude::*, widgets::Ui};
    let mut ui = Ui::new(100., 80.);
    let source = ui.signal(frame(false));
    let read = source.clone();
    let view = ui.mount(
        native_surface_signal("frame", move || read.get())
            .w(80.)
            .h(60.)
            .p(10.),
    );
    ui.prepare_frame();
    let viewport = ui.scene.borrow().children(view.node())[0];
    let image = ui.scene.borrow().children(viewport)[0];
    assert_eq!(
        ui.scene.borrow().bounds(viewport),
        Rect::new(10., 10., 60., 40.)
    );
    assert_eq!(
        ui.scene.borrow().bounds(image),
        Rect::new(20., 10., 40., 40.)
    );
    ui.scene.borrow_mut().flush();
    source.set(frame(true));
    ui.prepare_frame();
    assert_eq!(ui.scene.borrow_mut().flush().layout_nodes, 0);
    assert_eq!(ui.scene.borrow().children(viewport), &[image]);
    ui.remove(view.node());
    source.set(frame(false));
    assert!(!ui.scene.borrow().contains(image));
}
