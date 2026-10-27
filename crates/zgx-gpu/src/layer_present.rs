//! macOS presentation through IOSurfaces of the renderer's own, shown as a
//! plain layer's tiles instead of `CAMetalLayer` drawables (or, with
//! `ZGX_PRESENT_VIDEO`, at the display's full rate, queued by an
//! `AVSampleBufferDisplayLayer`).
//!
//! A drawable's contents are not kept between frames, so presenting one means
//! filling all of it: copying the whole retained frame every frame. A surface
//! of the renderer's own keeps what it held, so the retained frame can be the
//! surface itself: each frame repaints what changed since that surface was
//! last shown, and showing it copies nothing.
//!
//! Each surface is a tile of the plain layer, its contents set once; a frame
//! shows its surface's tile and hides the one before (see `Layer`). Handed
//! over at a set point before a refresh (the host's `refresh_grid` and its
//! alignment at full rate), tiles show one frame a refresh at the display's
//! full rate as reliably as a queue does, with no thread of their own. The
//! video layer, which queues each surface wrapped once in a sample buffer,
//! remains for a release behind `ZGX_PRESENT_VIDEO`, to compare with: its
//! CoreMedia thread wakes about 55 times a second while it lives, and its
//! queue holds a third surface.
use crate::{FORMAT, GpuError};
use objc2::runtime::ProtocolObject;
use objc2::{Encoding, Message, RefEncode, msg_send, rc::Retained, runtime::AnyObject};
use objc2_core_foundation::CGSize;
use objc2_core_foundation::{
    CFDictionary, CFNull, CFNumber, CFRetained, CFString, CFType, kCFNull,
};
use objc2_core_graphics::{CGColorSpace, kCGColorSpaceSRGB};
use objc2_io_surface::{
    IOSurfaceRef, kIOSurfaceBytesPerElement, kIOSurfaceColorSpace, kIOSurfaceHeight,
    kIOSurfacePixelFormat, kIOSurfaceWidth,
};
use objc2_metal::{MTLCommandBuffer, MTLCommandQueue, MTLDrawable, MTLTexture};
use objc2_metal::{
    MTLDevice, MTLPixelFormat, MTLStorageMode, MTLTextureDescriptor, MTLTextureType,
    MTLTextureUsage,
};
use objc2_quartz_core::{
    CAAutoresizingMask, CALayer, CAMetalDrawable, CAMetalLayer, CATransaction,
};
use std::{cell::Cell, ffi::c_void, ptr::null, rc::Rc};

/// A surface the renderer draws into and a layer shows.
pub(crate) struct Buffer {
    pub surface: CFRetained<IOSurfaceRef>,
    /// The surface as a sample buffer to enqueue (`Layer::show`), made the
    /// first time the video layer shows it: wrapping a surface sets up
    /// CoreVideo's pixel formats, a launch cost an app that never animates
    /// at full rate need not pay. `None` if it could not be made.
    sample: std::cell::OnceCell<Option<Sample>>,
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    /// The texture as Metal's, for frames encoded directly (`metal_frame`).
    pub metal: crate::metal_frame::Texture,
    /// Reading it to bring another surface up to date (copy_offset.wgsl).
    pub copy_source: std::cell::OnceCell<wgpu::BindGroup>,
    /// A `CAMetalLayer` drawable's (`Drawables`): presented by Metal, never a
    /// tile; the layer's pool reuses it.
    pub drawable: bool,
}
impl Buffer {
    /// The surface as a sample buffer, made on first use.
    fn sample(&self) -> Option<&Sample> {
        self.sample
            .get_or_init(|| Sample::new(&self.surface).ok())
            .as_ref()
    }
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Result<Self, GpuError> {
        // 'BGRA': four bytes a pixel, as `FORMAT` (Bgra8Unorm) stores them.
        let number = |value: i32| CFNumber::new_i32(value);
        let (w, h, bytes, format) = (
            number(width as i32),
            number(height as i32),
            number(4),
            number(i32::from_be_bytes(*b"BGRA")),
        );
        // SAFETY: The keys are IOSurface's own constant CFStrings.
        let keys: [&CFString; 4] = unsafe {
            [
                kIOSurfaceWidth,
                kIOSurfaceHeight,
                kIOSurfaceBytesPerElement,
                kIOSurfacePixelFormat,
            ]
        };
        let values: [&CFType; 4] = [&w, &h, &bytes, &format];
        let properties = CFDictionary::from_slices(&keys, &values);
        // SAFETY: The dictionary holds IOSurface creation properties, each a
        // CFNumber as IOSurface documents for these keys.
        let surface = unsafe { IOSurfaceRef::new(properties.as_opaque()) }
            .ok_or_else(|| GpuError("IOSurface creation failed".into()))?;
        // sRGB, as a `CAMetalLayer` without a colour space shows its drawables:
        // untagged layer contents would be taken as the display's space.
        // SAFETY: CoreGraphics' constant CFString, read only.
        if let Some(list) = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }))
            .and_then(|space| CGColorSpace::property_list(&space))
        {
            // SAFETY: IOSurface's colour-space key and a colour space's
            // property list form, as it documents.
            unsafe { surface.set_value(kIOSurfaceColorSpace, &list) };
        }
        // SAFETY: The raw device only makes a texture; nothing destroys it, and
        // the guard is dropped before wgpu is called again.
        let hal = unsafe { device.as_hal::<wgpu::hal::api::Metal>() }
            .ok_or_else(|| GpuError("not a Metal device".into()))?;
        // SAFETY: A plain 2D texture descriptor for a surface of this size.
        let descriptor = unsafe {
            MTLTextureDescriptor::texture2DDescriptorWithPixelFormat_width_height_mipmapped(
                MTLPixelFormat::BGRA8Unorm,
                width as usize,
                height as usize,
                false,
            )
        };
        descriptor.setUsage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
        descriptor.setStorageMode(MTLStorageMode::Shared);
        let metal = hal
            .raw_device()
            .newTextureWithDescriptor_iosurface_plane(&descriptor, &surface, 0)
            .ok_or_else(|| GpuError("Metal texture over an IOSurface failed".into()))?;
        drop(hal);
        Ok(Self::wrap(device, surface, metal, width, height, false))
    }
    /// A drawable's texture (and the surface under it) as a buffer.
    pub fn from_drawable(
        device: &wgpu::Device,
        surface: CFRetained<IOSurfaceRef>,
        metal: crate::metal_frame::Texture,
    ) -> Self {
        let (width, height) = (metal.width() as u32, metal.height() as u32);
        Self::wrap(device, surface, metal, width, height, true)
    }
    fn wrap(
        device: &wgpu::Device,
        surface: CFRetained<IOSurfaceRef>,
        metal: crate::metal_frame::Texture,
        width: u32,
        height: u32,
        drawable: bool,
    ) -> Self {
        // SAFETY: Metal made this texture on this device with this format and
        // size, one mip level and layer; it retains the IOSurface backing it.
        let raw = unsafe {
            wgpu::hal::metal::Device::texture_from_raw(
                metal.clone(),
                FORMAT,
                MTLTextureType::Type2D,
                1,
                1,
                wgpu::hal::CopyExtent {
                    width,
                    height,
                    depth: 1,
                },
            )
        };
        // SAFETY: Made from this device's raw device, as the descriptor says
        // (copies need no Metal usage flag). wgpu takes it as initialized:
        // its first frame copies in the whole shown one or repaints it all
        // (`begin_layer_frame`), and its memory is only ever read as pixels.
        let texture = unsafe {
            device.create_texture_from_hal::<wgpu::hal::api::Metal>(
                raw,
                &wgpu::TextureDescriptor {
                    label: Some("presented surface"),
                    size: wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: FORMAT,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING
                        | wgpu::TextureUsages::COPY_SRC
                        | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                },
            )
        };
        let view = texture.create_view(&Default::default());
        Self {
            surface,
            sample: std::cell::OnceCell::new(),
            texture,
            view,
            metal,
            copy_source: Default::default(),
            drawable,
        }
    }
}

/// A surface as a sample buffer a display layer can enqueue, made once: a
/// pixel buffer over the surface, tagged sRGB, marked to show at once.
struct Sample {
    pixels: *const c_void,
    sample: *const c_void,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct CMTime {
    value: i64,
    timescale: i32,
    flags: u32,
    epoch: i64,
}
#[repr(C)]
struct CMSampleTimingInfo {
    duration: CMTime,
    presentation: CMTime,
    decode: CMTime,
}
// SAFETY (these four blocks): each declaration matches its framework
// header: CF/CM/CV object references as untyped pointers, `Boolean` as
// `u8`, `CVAttachmentMode` as `u32`, OSStatus and CVReturn as `i32`, and
// `CMTime` and `CMSampleTimingInfo` laid out as above.
#[link(name = "CoreMedia", kind = "framework")]
unsafe extern "C" {
    static kCMTimeInvalid: CMTime;
    static kCMSampleAttachmentKey_DisplayImmediately: *const c_void;
    fn CMVideoFormatDescriptionCreateForImageBuffer(
        allocator: *const c_void,
        image: *const c_void,
        format: *mut *const c_void,
    ) -> i32;
    fn CMSampleBufferCreateReadyWithImageBuffer(
        allocator: *const c_void,
        image: *const c_void,
        format: *const c_void,
        timing: *const CMSampleTimingInfo,
        sample: *mut *const c_void,
    ) -> i32;
    fn CMSampleBufferGetSampleAttachmentsArray(sample: *const c_void, create: u8) -> *const c_void;
}
#[link(name = "CoreVideo", kind = "framework")]
unsafe extern "C" {
    static kCVImageBufferCGColorSpaceKey: *const c_void;
    static kCVImageBufferColorPrimariesKey: *const c_void;
    static kCVImageBufferColorPrimaries_ITU_R_709_2: *const c_void;
    static kCVImageBufferTransferFunctionKey: *const c_void;
    static kCVImageBufferTransferFunction_sRGB: *const c_void;
    fn CVPixelBufferCreateWithIOSurface(
        allocator: *const c_void,
        surface: *const c_void,
        attributes: *const c_void,
        pixels: *mut *const c_void,
    ) -> i32;
    fn CVBufferSetAttachment(
        buffer: *const c_void,
        key: *const c_void,
        value: *const c_void,
        mode: u32,
    );
    fn CVPixelBufferGetIOSurface(pixels: *const c_void) -> *const c_void;
}
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFBooleanTrue: *const c_void;
    fn CFArrayGetValueAtIndex(array: *const c_void, index: isize) -> *const c_void;
    fn CFDictionarySetValue(dictionary: *const c_void, key: *const c_void, value: *const c_void);
    fn CFRelease(object: *const c_void);
}
#[link(name = "AVFoundation", kind = "framework")]
unsafe extern "C" {
    static AVLayerVideoGravityResize: *const c_void;
}
impl Sample {
    fn new(surface: &IOSurfaceRef) -> Result<Self, GpuError> {
        let failed = |what: &str| GpuError(format!("{what} failed for a presented surface"));
        // SAFETY: CoreVideo and CoreMedia calls on objects made here, each
        // checked for failure before use; the pixel buffer retains the
        // surface. The created ones (+1) are released with `this`, on every
        // early return too, or (`format`) below. The attachments array is
        // borrowed (Get rule) and holds one mutable dictionary, as the sample
        // holds one image; the attachment keys and values are the
        // frameworks' constant CFStrings and CFBoolean.
        unsafe {
            let mut pixels = null();
            if CVPixelBufferCreateWithIOSurface(
                null(),
                (surface as *const IOSurfaceRef).cast(),
                null(),
                &mut pixels,
            ) != 0
                || pixels.is_null()
            {
                return Err(failed("CVPixelBufferCreateWithIOSurface"));
            }
            let mut this = Self {
                pixels,
                sample: null(),
            };
            // sRGB, as the surface itself is tagged (kCVAttachmentMode_ShouldPropagate).
            if let Some(space) = CGColorSpace::with_name(Some(kCGColorSpaceSRGB)) {
                let space = CFRetained::as_ptr(&space).as_ptr().cast::<c_void>();
                CVBufferSetAttachment(pixels, kCVImageBufferCGColorSpaceKey, space, 1);
            }
            CVBufferSetAttachment(
                pixels,
                kCVImageBufferColorPrimariesKey,
                kCVImageBufferColorPrimaries_ITU_R_709_2,
                1,
            );
            CVBufferSetAttachment(
                pixels,
                kCVImageBufferTransferFunctionKey,
                kCVImageBufferTransferFunction_sRGB,
                1,
            );
            let mut format = null();
            if CMVideoFormatDescriptionCreateForImageBuffer(null(), pixels, &mut format) != 0
                || format.is_null()
            {
                return Err(failed("CMVideoFormatDescriptionCreateForImageBuffer"));
            }
            // Shown on arrival, so its time does not matter.
            let zero = CMTime {
                value: 0,
                timescale: 1,
                flags: 1,
                epoch: 0,
            };
            let timing = CMSampleTimingInfo {
                duration: kCMTimeInvalid,
                presentation: zero,
                decode: kCMTimeInvalid,
            };
            let status = CMSampleBufferCreateReadyWithImageBuffer(
                null(),
                pixels,
                format,
                &timing,
                &mut this.sample,
            );
            CFRelease(format);
            if status != 0 || this.sample.is_null() {
                return Err(failed("CMSampleBufferCreateReadyWithImageBuffer"));
            }
            let attachments = CMSampleBufferGetSampleAttachmentsArray(this.sample, 1);
            if attachments.is_null() {
                return Err(failed("CMSampleBufferGetSampleAttachmentsArray"));
            }
            CFDictionarySetValue(
                CFArrayGetValueAtIndex(attachments, 0),
                kCMSampleAttachmentKey_DisplayImmediately,
                kCFBooleanTrue,
            );
            Ok(this)
        }
    }
}
impl Drop for Sample {
    fn drop(&mut self) {
        // SAFETY: Each was created (+1) in `new` and is released once.
        unsafe {
            if !self.sample.is_null() {
                CFRelease(self.sample);
            }
            CFRelease(self.pixels);
        }
    }
}

/// Layers showing the presented surface, filling the window's `CAMetalLayer`
/// (which follows the view's size and, given no drawables, shows nothing), or
/// another host layer (an overlay plane above native views).
///
/// While frames come at the display's full rate, a sample buffer display
/// layer queues them (see the module's documentation). Once it exists its
/// CoreMedia thread wakes about 55 times a second for good, even with
/// nothing to show, so slower frames, and a window at rest, are a plain
/// layer's contents instead, and the video layer is dropped at rest.
pub(crate) struct Layer {
    host: Retained<CALayer>,
    /// Holds a layer per surface (`tiles`), and no contents of its own.
    plain: Retained<CALayer>,
    /// Each surface shown on the plain layer, as a sublayer of `plain` whose
    /// contents are set once, when the surface is first shown: showing a
    /// surface again flips which tile is hidden. New contents every frame
    /// make the window server take the surface in afresh each time, which
    /// cost it about 4 points of a core more than GPUI's presents in the
    /// heavy dashboard (2026-10-25: 13.4% against 9.8% with tiles, at the
    /// same app CPU and GPU power). Dropped with their surfaces (`keep`).
    tiles: Vec<(u32, Retained<CALayer>)>,
    /// Whether `plain` has contents (shows a surface, or holds one under
    /// `video` until the video layer shows a frame of its own).
    plain_shows: Rc<Cell<bool>>,
    /// Counts changes of which layer shows frames, so a pending `Uncover`
    /// of an earlier one does nothing.
    generation: Rc<Cell<u64>>,
    /// An `Uncover` is pending.
    uncovering: Rc<Cell<bool>>,
    video: Option<Video>,
    /// What the layers were last given, so a frame sets only what changed.
    opaque: Cell<bool>,
    /// The caret the host draws itself (`Layer::set_caret`), above the
    /// frames, and what it was last given.
    caret: Option<(Retained<CALayer>, Option<Caret>)>,
    /// Full-rate frames presented as `CAMetalLayer` drawables
    /// (`ZGX_PRESENT_DRAWABLE`), above the plain layer.
    drawables: Option<Drawables>,
    /// While the full-rate trial judges tiles by frames lost
    /// (`ZGX_TRIAL_RULE=frames`): a 1×1 drawable presented in each tile
    /// flip's transaction, whose presented time says when that flip reached
    /// the screen (tiles have no feedback of their own).
    probe: Option<Probe>,
}
/// A caret drawn by Core Animation over the window's frames
/// (`zgx::caret_blink::NativeCaret`): a box of one colour, shown or hidden as
/// the document's blink says, so the window draws no frame for a blink.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Caret {
    /// Its box in the host layer's points, from the top left.
    pub rect: (f64, f64, f64, f64),
    /// Its colour, sRGB, each 0 to 1.
    pub color: [f64; 4],
    /// Whether it shows now (a blink's phase).
    pub shown: bool,
}
/// See `Layer::probe`.
struct Probe {
    layer: Retained<CAMetalLayer>,
    queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,
    feedback: std::sync::Arc<Feedback>,
}
impl Probe {
    fn new(
        host: &CALayer,
        device: &ProtocolObject<dyn objc2_metal::MTLDevice>,
        queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,
    ) -> Self {
        let layer = CAMetalLayer::new();
        layer.setDevice(Some(device));
        layer.setPixelFormat(MTLPixelFormat::BGRA8Unorm);
        layer.setPresentsWithTransaction(true);
        layer.setDrawableSize(CGSize::new(1., 1.));
        layer.setOpaque(false);
        layer.setFrame(objc2_core_foundation::CGRect::new(
            objc2_core_foundation::CGPoint::new(0., 0.),
            CGSize::new(1., 1.),
        ));
        transaction(|| host.addSublayer(&layer));
        crate::present_log::log(|| "tile-probe-start".into());
        Self {
            layer,
            queue,
            feedback: Default::default(),
        }
    }
    /// Present a cleared (transparent) drawable in the caller's transaction,
    /// its work scheduled first, recording when it showed.
    fn present(&self) {
        let Some(drawable) = self.layer.nextDrawable() else {
            return;
        };
        let Some(buffer) = self.queue.commandBuffer() else {
            return;
        };
        let pass = objc2_metal::MTLRenderPassDescriptor::new();
        // SAFETY: Index 0 of a render pass descriptor's colour attachments.
        let color = unsafe { pass.colorAttachments().objectAtIndexedSubscript(0) };
        color.setTexture(Some(&drawable.texture()));
        color.setLoadAction(objc2_metal::MTLLoadAction::Clear);
        color.setClearColor(objc2_metal::MTLClearColor {
            red: 0.,
            green: 0.,
            blue: 0.,
            alpha: 0.,
        });
        color.setStoreAction(objc2_metal::MTLStoreAction::Store);
        if let Some(encoder) = buffer.renderCommandEncoderWithDescriptor(&pass) {
            use objc2_metal::MTLCommandEncoder;
            encoder.endEncoding();
        }
        let feedback = self.feedback.clone();
        let handler = block2::RcBlock::new(
            move |shown: std::ptr::NonNull<ProtocolObject<dyn MTLDrawable>>| {
                // SAFETY: Metal passes the drawable presented, alive for the call.
                feedback.record(unsafe { shown.as_ref() }.presentedTime());
            },
        );
        // SAFETY: A live block taking the drawable, as the handler's type
        // declares; Metal copies it.
        unsafe { drawable.addPresentedHandler(block2::RcBlock::as_ptr(&handler)) };
        buffer.commit();
        buffer.waitUntilScheduled();
        drawable.present();
    }
}
/// When the window server showed each drawable (`addPresentedHandler`), as
/// the gaps between successive presented drawables and the count it skipped:
/// what the screen did, not what the renderer handed over.
#[derive(Default)]
pub(crate) struct Feedback {
    state: std::sync::Mutex<FeedbackState>,
}
#[derive(Default)]
struct FeedbackState {
    presented: u64,
    skipped: u64,
    /// The last presented time (host seconds), and the latest gaps (ms).
    last: f64,
    gaps: std::collections::VecDeque<f64>,
}
/// A point in a `Feedback`'s record (`Feedback::mark`).
#[derive(Clone, Copy, Default)]
pub(crate) struct Mark {
    presented: u64,
    skipped: u64,
}
impl Feedback {
    fn record(&self, presented: f64) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if presented <= 0. {
            state.skipped += 1;
            return;
        }
        state.presented += 1;
        if state.last > 0. && presented > state.last {
            let gap = (presented - state.last) * 1e3;
            state.gaps.push_back(gap);
            if state.gaps.len() > 600 {
                state.gaps.pop_front();
            }
        }
        state.last = presented;
    }
    /// Where the record stands now.
    pub(crate) fn mark(&self) -> Mark {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        Mark {
            presented: state.presented,
            skipped: state.skipped,
        }
    }
    /// Since `from`: frames handed over (shown or skipped), and those the
    /// window server skipped. (Gaps between shown ones are not counted: the
    /// probe's presented times come with its transaction's, which batch.)
    pub(crate) fn lost_since(&self, from: Mark) -> (u64, u64) {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let skipped = state.skipped - from.skipped;
        (state.presented - from.presented + skipped, skipped)
    }
    /// Drawables presented and skipped so far, and the latest gaps' median,
    /// 99th percentile and count over 1.5 of the median (a refresh missed).
    pub(crate) fn summary(&self) -> (u64, u64, f64, f64, usize) {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let mut gaps: Vec<f64> = state.gaps.iter().copied().collect();
        gaps.sort_by(f64::total_cmp);
        let at = |q: f64| {
            gaps.get(((gaps.len() as f64 - 1.) * q) as usize)
                .copied()
                .unwrap_or(0.)
        };
        let median = at(0.5);
        let late = gaps.iter().filter(|gap| **gap > median * 1.5).count();
        (state.presented, state.skipped, median, at(0.99), late)
    }
}
/// A `CAMetalLayer` whose drawables are the surfaces full-rate frames draw
/// into, presented by Metal, which hands each to the window server for the
/// refresh after its work completes: the system schedules them, as GPUI's.
/// Each drawable's surface keeps what it held, so a frame repaints what
/// changed since that drawable was last presented (`Buffer::drawable`).
struct Drawables {
    layer: Retained<CAMetalLayer>,
    /// The drawable the frame being drawn presents.
    pending: Option<Retained<ProtocolObject<dyn CAMetalDrawable>>>,
    /// Whether the layer shows (its frames, over the hidden tiles).
    shown: bool,
    /// When each drawable was shown (`Feedback`).
    feedback: std::sync::Arc<Feedback>,
}
/// Hides the plain layer's tiles once the video layer above it shows a
/// frame: until then they show the frame before; after, under a window's
/// translucent pixels, they would show through. Checked on the main queue
/// each refresh or so, as no further frame may come to check it.
struct Uncover {
    plain: Retained<CALayer>,
    /// The video layer, kept all but transparent over a translucent window
    /// until now (see `Layer::show`): shown in the transaction that clears
    /// `plain`, so the two never show one frame twice.
    video: Retained<CALayer>,
    queue: Retained<AnyObject>,
    reports_displayed: bool,
    shows: Rc<Cell<bool>>,
    generation: Rc<Cell<u64>>,
    expected: u64,
    pending: Rc<Cell<bool>>,
    tries: u32,
}
impl Uncover {
    fn schedule(self: Box<Self>) {
        // SAFETY: The main queue runs `run` once with the box, which it
        // takes back. The box is not `Send` (its `Rc`s are shared with the
        // `Layer`): sound because a `Layer`, itself not `Send`, exists only
        // on the main thread (`Layer::attach` asserts it), so the main queue
        // runs `run` on the thread that made it.
        unsafe {
            dispatch_after_f(
                dispatch_time(0, 8_000_000),
                &raw const _dispatch_main_q,
                Box::into_raw(self).cast(),
                Self::run,
            )
        };
    }
    extern "C" fn run(context: *mut c_void) {
        // SAFETY: `schedule` passed a boxed `Uncover`, for this call alone.
        let mut this = unsafe { Box::from_raw(context.cast::<Uncover>()) };
        if this.generation.get() != this.expected {
            this.pending.set(false);
            return;
        }
        if !this.shows.get() {
            transaction(|| this.video.setOpacity(1.));
            this.pending.set(false);
            return;
        }
        this.tries += 1;
        // Without word from the video layer, about six refreshes.
        let shown = if this.reports_displayed {
            displayed(&this.queue).is_some()
        } else {
            this.tries > 6
        };
        if shown || this.tries > 60 {
            crate::present_log::log(|| format!("uncover tries={} confirmed={shown}", this.tries));
            transaction(|| {
                hide_tiles(&this.plain, None);
                this.video.setOpacity(1.);
            });
            this.shows.set(false);
            this.pending.set(false);
            return;
        }
        this.schedule();
    }
}
/// `ZGX_PRESENT_LOG`: every 4 ms on the main queue, the presented state of
/// `host` and its layers as Core Animation holds it (not as zgx believes
/// it set it), logged when it changes.
struct Sampler {
    host: Retained<CALayer>,
    last: String,
}
impl Sampler {
    fn schedule(self: Box<Self>) {
        // SAFETY: The main queue runs `run` once with the box, which it takes
        // back; scheduled from `Layer::attach`, on the main thread, so the
        // box (not `Send`) stays on it.
        unsafe {
            dispatch_after_f(
                dispatch_time(0, 4_000_000),
                &raw const _dispatch_main_q,
                Box::into_raw(self).cast(),
                Self::run,
            )
        };
    }
    extern "C" fn run(context: *mut c_void) {
        // SAFETY: `schedule` passed a boxed `Sampler`, for this call alone.
        let mut this = unsafe { Box::from_raw(context.cast::<Sampler>()) };
        let state = describe(&this.host);
        if state != this.last {
            crate::present_log::log(|| format!("layers {state}"));
            this.last = state;
        }
        this.schedule();
    }
}
fn describe(host: &CALayer) -> String {
    let one = |layer: &CALayer| {
        // SAFETY: `contents` returns a nullable object (`id`), retained here.
        let contents: Option<Retained<AnyObject>> = unsafe { msg_send![layer, contents] };
        format!(
            "{}(op={:.3} hidden={} contents={})",
            layer.class().name().to_str().unwrap_or("?"),
            layer.opacity(),
            layer.isHidden(),
            contents.map_or("none".into(), |c| format!("{:p}", &*c)),
        )
    };
    let mut out = format!("host[{}]", one(host));
    let mut ancestor = host.superlayer();
    let mut depth = 0;
    while let Some(layer) = ancestor {
        if layer.isHidden() || layer.opacity() < 1. {
            out += &format!(" ancestor{depth}[{}]", one(&layer));
        }
        ancestor = layer.superlayer();
        depth += 1;
    }
    // SAFETY: The layer's live sublayer array, not changed while it is read
    // here (on the main thread, nothing else running).
    if let Some(subs) = unsafe { host.sublayers() } {
        for sub in subs.iter() {
            out += " ";
            out += &one(&sub);
            // The plain layer's tiles, one per surface (`Layer::tiles`).
            // SAFETY: as the host's, above.
            if let Some(tiles) = unsafe { sub.sublayers() } {
                out += "{";
                out += &tiles
                    .iter()
                    .map(|tile| one(&tile))
                    .collect::<Vec<_>>()
                    .join(" ");
                out += "}";
            }
        }
    }
    out
}
// SAFETY: libdispatch's declarations: the main queue's object (only its
// address is taken), `dispatch_time_t` as `u64`, and a plain C work function.
#[link(name = "System", kind = "dylib")]
unsafe extern "C" {
    static _dispatch_main_q: c_void;
    fn dispatch_time(when: u64, delta: i64) -> u64;
    fn dispatch_after_f(
        when: u64,
        queue: *const c_void,
        context: *mut c_void,
        work: extern "C" fn(*mut c_void),
    );
}
/// `CMSampleBufferRef` and `CVPixelBufferRef` pointees, encoded as the
/// Objective-C runtime declares them (objc2 checks messages in debug builds).
#[repr(C)]
struct CMSampleBuffer([u8; 0]);
// SAFETY: `opaqueCMSampleBuffer` is CoreMedia's struct tag for the type.
unsafe impl RefEncode for CMSampleBuffer {
    const ENCODING_REF: Encoding =
        Encoding::Pointer(&Encoding::Struct("opaqueCMSampleBuffer", &[]));
}
#[repr(C)]
struct CVBuffer([u8; 0]);
// SAFETY: `__CVBuffer` is CoreVideo's struct tag for the type.
unsafe impl RefEncode for CVBuffer {
    const ENCODING_REF: Encoding = Encoding::Pointer(&Encoding::Struct("__CVBuffer", &[]));
}
/// The surface a video renderer shows (its IOSurface ID), if any.
fn displayed(queue: &AnyObject) -> Option<u32> {
    // SAFETY: Returns a retained CVPixelBufferRef or null, released here; the
    // surface it names is borrowed only for its ID.
    unsafe {
        let pixels: *mut CVBuffer = msg_send![queue, copyDisplayedPixelBuffer];
        let pixels = pixels.cast::<c_void>();
        if pixels.is_null() {
            return None;
        }
        let surface = CVPixelBufferGetIOSurface(pixels);
        let id = (!surface.is_null()).then(|| (*surface.cast::<IOSurfaceRef>()).id());
        CFRelease(pixels);
        id
    }
}
struct Video {
    layer: Retained<CALayer>,
    /// What surfaces are enqueued to: the layer's video renderer (macOS 14),
    /// or the layer itself.
    queue: Retained<AnyObject>,
    /// Whether `queue` can say what it shows (`displayed`).
    reports_displayed: bool,
}
/// Fill `host` and follow its size, without implicit animation.
fn fill(layer: &CALayer, host: &CALayer, opaque: bool) {
    layer.setFrame(host.bounds());
    layer.setContentsScale(host.contentsScale());
    layer.setOpaque(opaque);
    layer.setAutoresizingMask(
        CAAutoresizingMask::LayerWidthSizable | CAAutoresizingMask::LayerHeightSizable,
    );
    let keys = [
        "contents",
        "contentsScale",
        "opaque",
        "bounds",
        "position",
        "hidden",
    ]
    .map(CFString::from_static_str);
    // SAFETY: CoreFoundation's null singleton.
    let null_value: &CFNull = unsafe { kCFNull }.expect("kCFNull");
    let actions = CFDictionary::<CFString, CFNull>::from_slices(
        &keys.each_ref().map(|key| &**key),
        &[null_value; 6],
    );
    // SAFETY: A CFDictionary is toll-free bridged to NSDictionary, and
    // CFNull to NSNull: an action that does nothing.
    unsafe {
        let actions = &*(&*actions as *const CFDictionary<CFString, CFNull>).cast::<AnyObject>();
        let _: () = msg_send![layer, setActions: actions];
    }
}
/// Shows `plain`'s tile of the surface `shown` (none: hides them all),
/// changing only the tiles whose state differs.
fn hide_tiles(plain: &CALayer, shown: Option<&CALayer>) {
    // SAFETY: The layer's live sublayer array, read on the main thread while
    // nothing else changes it.
    if let Some(tiles) = unsafe { plain.sublayers() } {
        for tile in tiles.iter() {
            let hidden = shown.is_none_or(|shown| !std::ptr::eq(&*tile, shown));
            if tile.isHidden() != hidden {
                tile.setHidden(hidden);
            }
        }
    }
}
/// Run `change` as one Core Animation transaction, without animation.
fn transaction(change: impl FnOnce()) {
    CATransaction::begin();
    CATransaction::setDisableActions(true);
    change();
    CATransaction::commit();
}
impl Video {
    fn new(host: &CALayer, opaque: bool, hidden: bool) -> Self {
        // SAFETY: AVFoundation (linked above) defines the class; `new` makes
        // a layer, a CALayer subclass.
        let layer: Retained<CALayer> = unsafe {
            let layer: Retained<AnyObject> =
                msg_send![objc2::class!(AVSampleBufferDisplayLayer), new];
            Retained::cast_unchecked(layer)
        };
        // SAFETY: The layer's own properties, with an AVLayerVideoGravity
        // constant: stretch each surface (the window's size in pixels) to the
        // layer's bounds, so one pixel is one pixel.
        unsafe {
            let gravity = &*AVLayerVideoGravityResize.cast::<AnyObject>();
            let _: () = msg_send![&*layer, setVideoGravity: gravity];
            // A window animating is not a video playing: the display sleeps
            // as it otherwise would.
            let _: () = msg_send![&*layer, setPreventsDisplaySleepDuringVideoPlayback: false];
        }
        fill(&layer, host, opaque);
        // Not 0: a layer the window server skips shows no frame.
        layer.setOpacity(if hidden { 0.001 } else { 1. });
        // SAFETY: `sampleBufferRenderer` (macOS 14) returns the layer's
        // renderer; earlier systems enqueue to the layer itself.
        let queue = unsafe {
            let has_renderer: bool =
                msg_send![&*layer, respondsToSelector: objc2::sel!(sampleBufferRenderer)];
            let renderer: Option<Retained<AnyObject>> = if has_renderer {
                msg_send![&*layer, sampleBufferRenderer]
            } else {
                None
            };
            renderer.unwrap_or_else(|| Retained::cast_unchecked(layer.clone()))
        };
        // SAFETY: A selector check on a live object.
        let reports_displayed: bool = unsafe {
            msg_send![&*queue, respondsToSelector: objc2::sel!(copyDisplayedPixelBuffer)]
        };
        // Above the plain layer, which shows the frame before until this
        // layer shows one.
        transaction(|| host.addSublayer(&layer));
        Self {
            layer,
            queue,
            reports_displayed,
        }
    }
    fn displayed(&self) -> Option<u32> {
        self.reports_displayed
            .then(|| displayed(&self.queue))
            .flatten()
    }
    fn enqueue(&self, sample: &Sample) {
        // SAFETY: The renderer's (or layer's) own methods; the sample buffer
        // is a live CMSampleBufferRef the buffer owns.
        unsafe {
            // A renderer that failed, or was interrupted, shows nothing new
            // until flushed (AVQueuedSampleBufferRenderingStatusFailed is 2).
            let status: isize = msg_send![&*self.queue, status];
            let interrupted: bool = msg_send![&*self.queue, requiresFlushToResumeDecoding];
            if status == 2 || interrupted {
                let _: () = msg_send![&*self.queue, flush];
            }
            let sample = sample.sample.cast_mut().cast::<CMSampleBuffer>();
            let _: () = msg_send![&*self.queue, enqueueSampleBuffer: sample];
        }
    }
}
impl Layer {
    pub fn attach(host: &CALayer) -> Self {
        // `Uncover` and `Sampler` share this layer's `Rc`s with work on the
        // main queue: the layer (not `Send`) must live on the main thread.
        assert!(
            objc2::MainThreadMarker::new().is_some(),
            "layer presentation runs on the main thread"
        );
        let plain = CALayer::new();
        fill(&plain, host, false);
        transaction(|| host.addSublayer(&plain));
        if crate::present_log::enabled() {
            Box::new(Sampler {
                host: host.retain(),
                last: String::new(),
            })
            .schedule();
        }
        Self {
            host: host.retain(),
            plain,
            plain_shows: Default::default(),
            generation: Default::default(),
            uncovering: Default::default(),
            video: None,
            tiles: Vec::new(),
            opaque: Cell::new(false),
            caret: None,
            drawables: None,
            probe: None,
        }
    }
    /// Start the tile probe (`Layer::probe`), if not yet.
    pub fn start_probe(
        &mut self,
        device: &ProtocolObject<dyn objc2_metal::MTLDevice>,
        queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,
    ) {
        if self.probe.is_none() {
            self.probe = Some(Probe::new(&self.host, device, queue));
        }
    }
    /// Stop the tile probe and drop its layer.
    pub fn stop_probe(&mut self) {
        if let Some(probe) = self.probe.take() {
            transaction(|| probe.layer.removeFromSuperlayer());
            crate::present_log::log(|| "tile-probe-stop".into());
        }
    }
    /// What the tile probe saw, if it runs.
    pub fn probe_feedback(&self) -> Option<std::sync::Arc<Feedback>> {
        self.probe.as_ref().map(|probe| probe.feedback.clone())
    }
    /// The next drawable for a full-rate frame of `width` by `height`
    /// pixels: its texture and the surface under it (the layer made, above
    /// the plain one, on first use). `None` if Metal gives none.
    pub fn next_drawable(
        &mut self,
        device: &ProtocolObject<dyn objc2_metal::MTLDevice>,
        width: u32,
        height: u32,
        feedback: bool,
    ) -> Option<(crate::metal_frame::Texture, CFRetained<IOSurfaceRef>)> {
        let opaque = self.opaque.get();
        let drawables = self.drawables.get_or_insert_with(|| {
            let layer = CAMetalLayer::new();
            layer.setDevice(Some(device));
            layer.setPixelFormat(MTLPixelFormat::BGRA8Unorm);
            // Read as the shown frame to bring other surfaces up to date.
            layer.setFramebufferOnly(false);
            // Metal presents each when its work completes, not in a
            // transaction: no commit a frame.
            layer.setPresentsWithTransaction(false);
            // Three: with 2 the pool is full and
            // the window server hands every image back at once, which cost it
            // 3-4 points of CPU per window (2026-10-26, M5, paired seconds
            // against tiles: heavy stream +4.4 with 2, -0.3 with 3; animate
            // +2.8, 0.0).
            layer.setMaximumDrawableCount(3);
            // sRGB, as the tiles' surfaces are tagged.
            // SAFETY: CoreGraphics' constant CFString, read only.
            if let Some(space) = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB })) {
                layer.setColorspace(Some(&space));
            }
            fill(&layer, &self.host, opaque);
            layer.setHidden(true);
            transaction(|| self.host.addSublayer(&layer));
            crate::present_log::log(|| "drawables-start".into());
            Drawables {
                layer,
                pending: None,
                shown: false,
                feedback: Default::default(),
            }
        });
        let size = CGSize::new(width as f64, height as f64);
        if drawables.layer.drawableSize() != size {
            drawables.layer.setDrawableSize(size);
        }
        if drawables.layer.isOpaque() != opaque {
            transaction(|| drawables.layer.setOpaque(opaque));
        }
        let drawable = drawables.layer.nextDrawable()?;
        // Feedback only while it is read (the trial, a log): a handler a
        // frame is work on Core Animation's thread.
        if feedback {
            let feedback = drawables.feedback.clone();
            let handler = block2::RcBlock::new(
                move |shown: std::ptr::NonNull<ProtocolObject<dyn MTLDrawable>>| {
                    // SAFETY: Metal passes the drawable presented, alive for the call.
                    feedback.record(unsafe { shown.as_ref() }.presentedTime());
                },
            );
            // SAFETY: A live block taking the drawable, as the handler's type
            // declares; Metal copies it.
            unsafe { drawable.addPresentedHandler(block2::RcBlock::as_ptr(&handler)) };
        }
        let texture = drawable.texture();
        let surface = texture.iosurface()?;
        drawables.pending = Some(drawable);
        Some((texture, surface.into()))
    }
    /// The pending drawable, for the frame's own command buffer to present
    /// (`metal_frame::Frame::commit_presenting`).
    pub fn take_drawable(&mut self) -> Option<Retained<ProtocolObject<dyn CAMetalDrawable>>> {
        self.drawables.as_mut()?.pending.take()
    }
    /// Present the frame drawn into the pending drawable, if its command
    /// buffer did not (a frame wgpu encoded), after the work already on
    /// `queue`; the drawable layer shows over the hidden tiles.
    pub fn present_drawable(&mut self, queue: Option<&ProtocolObject<dyn MTLCommandQueue>>) {
        let Some(drawables) = &mut self.drawables else {
            return;
        };
        if let Some(drawable) = drawables.pending.take()
            && let Some(buffer) = queue.and_then(|queue| queue.commandBuffer())
        {
            buffer.presentDrawable(ProtocolObject::<dyn MTLDrawable>::from_ref(&*drawable));
            buffer.commit();
        }
        if !drawables.shown {
            drawables.shown = true;
            let layer = drawables.layer.clone();
            transaction(|| {
                layer.setHidden(false);
                hide_tiles(&self.plain, None);
            });
            self.plain_shows.set(false);
        }
    }
    /// What the window server did with the drawables (`Feedback`), if any.
    pub fn drawable_feedback(&self) -> Option<std::sync::Arc<Feedback>> {
        self.drawables
            .as_ref()
            .map(|drawables| drawables.feedback.clone())
    }
    /// Hide the drawable layer (the plain layer shows frames again), in the
    /// caller's transaction.
    fn hide_drawables(&mut self) {
        if let Some(drawables) = &mut self.drawables
            && drawables.shown
        {
            drawables.shown = false;
            drawables.layer.setHidden(true);
        }
    }
    /// Drop the drawable layer and its pool (at rest, showing a tile).
    pub fn release_drawables(&mut self) {
        if let Some(drawables) = self.drawables.take() {
            transaction(|| drawables.layer.removeFromSuperlayer());
            crate::present_log::log(|| "drawables-drop".into());
        }
    }
    /// Show `caret` over the frames, or none; in the caller's transaction
    /// (the frame's, so the editor's caret and this one never both show,
    /// nor neither), or one of its own.
    pub fn set_caret(&mut self, caret: Option<Caret>) {
        if self.caret.as_ref().map(|(_, shown)| *shown) == Some(caret)
            || (self.caret.is_none() && caret.is_none())
        {
            return;
        }
        transaction(|| {
            let host = &self.host;
            let (layer, shown) = self.caret.get_or_insert_with(|| {
                let layer = CALayer::new();
                fill(&layer, host, true);
                // Its own box, not the host's.
                layer.setAutoresizingMask(CAAutoresizingMask::empty());
                layer.setHidden(true);
                // Above the frames, whichever layer shows them.
                layer.setZPosition(1.);
                host.addSublayer(&layer);
                (layer, None)
            });
            let Some(caret) = caret else {
                layer.setHidden(true);
                *shown = None;
                return;
            };
            let (x, y, width, height) = caret.rect;
            // A host whose geometry is not flipped counts from the bottom.
            let y = if host.isGeometryFlipped() || host.contentsAreFlipped() {
                y
            } else {
                host.bounds().size.height - y - height
            };
            let frame = objc2_core_foundation::CGRect::new(
                objc2_core_foundation::CGPoint::new(x, y),
                objc2_core_foundation::CGSize::new(width, height),
            );
            if layer.frame() != frame {
                layer.setFrame(frame);
            }
            crate::present_log::log(|| format!("caret frame {frame:?}"));
            if shown.is_none_or(|was| was.color != caret.color) {
                let [r, g, b, a] = caret.color;
                let color = objc2_core_graphics::CGColor::new_srgb(r, g, b, a);
                layer.setBackgroundColor(Some(&color));
            }
            // A blink's phase, by its opacity (window 28, 2026-10-27: two such
            // changes a second cost the window server what GPUI's blinking
            // frames do, and a Core Animation blink a quarter point more).
            let opacity = if caret.shown { 1. } else { 0. };
            if layer.opacity() != opacity {
                layer.setOpacity(opacity);
            }
            layer.setHidden(false);
            *shown = Some(caret);
        });
    }
    /// Shows `buffer`'s tile on the plain layer, made (its contents set) the
    /// first time the surface shows; in the caller's transaction.
    fn show_tile(&mut self, buffer: &Buffer) {
        // The plain layer exactly the host's box too: autoresizing rounds it
        // as it does a tile (a host that starts empty and grows to 1168.5 by
        // 400.25 points leaves it 1169 by 401), and every tile would show
        // its surface stretched to that.
        let host = self.host.bounds();
        if self.plain.frame() != host {
            self.plain.setFrame(host);
        }
        let id = buffer.surface.id();
        let tile = match self.tiles.iter().position(|(tile, _)| *tile == id) {
            Some(at) => &self.tiles[at].1,
            None => {
                let tile = CALayer::new();
                fill(&tile, &self.plain, self.opaque.get());
                tile.setHidden(true);
                let surface = &*buffer.surface as *const IOSurfaceRef as *const AnyObject;
                // SAFETY: `surface` points to the buffer's live IOSurface, an
                // object (bridged to `IOSurface`) and a layer's valid contents,
                // which the layer retains.
                unsafe { tile.setContents(Some(&*surface)) };
                self.plain.addSublayer(&tile);
                self.tiles.push((id, tile));
                &self.tiles[self.tiles.len() - 1].1
            }
        };
        // Exactly the plain layer's box, one surface pixel a screen pixel.
        // Autoresizing rounds to whole points: a tile made while the host
        // was empty and grown to 1168.5 points wide ends up 1169, its
        // surface stretched a pixel wider than the others' as they take
        // turns showing. Every tile, so one hidden now shows exact too.
        let bounds = self.plain.bounds();
        for (_, each) in &self.tiles {
            if each.frame() != bounds {
                each.setFrame(bounds);
            }
        }
        hide_tiles(&self.plain, Some(tile));
        self.hide_drawables();
        if let Some(probe) = &self.probe {
            probe.present();
        }
    }
    /// Drops the tiles of surfaces other than `surfaces` (the renderer's, as
    /// it keeps fewer or makes new ones): a tile holds its surface.
    pub fn keep(&mut self, surfaces: impl Iterator<Item = u32> + Clone) {
        if self
            .tiles
            .iter()
            .all(|(id, _)| surfaces.clone().any(|kept| kept == *id))
        {
            return;
        }
        transaction(|| {
            self.tiles.retain(|(id, tile)| {
                let kept = surfaces.clone().any(|kept| kept == *id);
                if !kept {
                    tile.removeFromSuperlayer();
                }
                kept
            })
        });
    }
    /// The surface on screen (its IOSurface ID), if the video layer shows it
    /// and says so (macOS 14.4). The window server keeps every surface a
    /// video layer was given marked in use, so `IOSurfaceIsInUse` cannot
    /// tell; a plain layer's contents release theirs once replaced.
    pub fn displayed(&self) -> Option<u32> {
        self.video.as_ref().and_then(Video::displayed)
    }
    /// Show `buffer`'s surface: queued by the video layer when `steady`
    /// (frames come at the display's full rate), else as the plain layer's
    /// contents. `opaque`:
    /// every pixel is, so the window server need not blend the layers over
    /// what lies behind them.
    pub fn show(&mut self, buffer: &Buffer, opaque: bool, steady: bool) {
        // The video layer queues a sample buffer; a surface none can wrap
        // shows as the plain layer's.
        let sample = match steady || self.video.is_some() {
            true => match buffer.sample() {
                Some(sample) => Some(sample),
                None => return self.show_plain(buffer, opaque),
            },
            false => None,
        };
        self.set_opaque(opaque);
        if steady && self.video.is_none() {
            crate::present_log::log(|| {
                format!(
                    "video-start opaque={opaque} plain_shows={}",
                    self.plain_shows.get()
                )
            });
            // Over a translucent window, the plain layer's frame and the
            // video layer's would blend twice (the window darkens for a
            // refresh) between the video layer's first frame and clearing
            // the plain one: it starts all but transparent (still drawn, so
            // it says what it shows) and `Uncover` shows it as the plain
            // layer clears. An opaque frame covers the one under it.
            let hidden = !opaque && self.plain_shows.get();
            self.video = Some(Video::new(&self.host, opaque, hidden));
        }
        if self.video.is_none() {
            crate::present_log::log(|| format!("plain-set surface={}", buffer.surface.id()));
            transaction(|| self.show_tile(buffer));
            self.plain_shows.set(true);
            return;
        }
        let video = self.video.as_ref().expect("checked above");
        video.enqueue(sample.expect("looked for while the video layer shows"));
        if !self.plain_shows.get() || self.uncovering.replace(true) {
            return;
        }
        Box::new(Uncover {
            plain: self.plain.clone(),
            video: video.layer.clone(),
            queue: video.queue.clone(),
            reports_displayed: video.reports_displayed,
            shows: self.plain_shows.clone(),
            generation: self.generation.clone(),
            expected: self.generation.get(),
            pending: self.uncovering.clone(),
            tries: 0,
        })
        .schedule();
    }
    /// Show `buffer` on the plain layer now, dropping the video layer. Its
    /// contents change in the caller's Core Animation transaction, so during
    /// a window resize the frame lands with the new window size; the video
    /// layer shows frames on its own thread, a refresh or more later, laid
    /// out against bounds that have moved on (content shifts a few points,
    /// or edges show gaps).
    pub fn show_plain(&mut self, buffer: &Buffer, opaque: bool) {
        self.set_opaque(opaque);
        let video = self.video.take();
        if video.is_some() {
            crate::present_log::log(|| "video-drop (plain takes over)".into());
            self.generation.set(self.generation.get() + 1);
        }
        crate::present_log::log(|| {
            format!("plain-set surface={} (show_plain)", buffer.surface.id())
        });
        transaction(|| {
            self.show_tile(buffer);
            if let Some(video) = &video {
                video.layer.removeFromSuperlayer();
            }
        });
        self.plain_shows.set(true);
    }
    fn set_opaque(&self, opaque: bool) {
        if self.opaque.replace(opaque) != opaque {
            transaction(|| {
                for (_, tile) in &self.tiles {
                    tile.setOpaque(opaque);
                }
                if let Some(video) = &self.video {
                    video.layer.setOpaque(opaque);
                }
            });
        }
    }
    /// The window is at rest showing `shown`: show it on the plain layer
    /// and drop the video layer.
    pub fn rest(&mut self, shown: &Buffer) {
        if self.video.is_some() {
            let opaque = self.opaque.get();
            self.show_plain(shown, opaque);
        }
    }
}
impl Drop for Layer {
    fn drop(&mut self) {
        self.generation.set(self.generation.get() + 1);
        transaction(|| {
            if let Some(video) = &self.video {
                video.layer.removeFromSuperlayer();
            }
            self.plain.removeFromSuperlayer();
            if let Some(drawables) = &self.drawables {
                drawables.layer.removeFromSuperlayer();
            }
            if let Some(probe) = &self.probe {
                probe.layer.removeFromSuperlayer();
            }
        });
    }
}
