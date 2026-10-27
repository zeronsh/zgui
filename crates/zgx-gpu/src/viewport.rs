//! GPU views (`zgx::gpu_view`): an app's own wgpu drawing, composited as an
//! image.
//!
//! A view's frame is an image with no pixels of its own: the first time a
//! renderer draws one (its image cache misses), it has the view's painter
//! draw it into the view's texture, submitted to the queue ahead of the
//! frame that samples it, and caches that texture under the frame's id in
//! place of the view's last one. So a view costs nothing while it is not
//! invalidated, and nothing at all where none is shown: the cache's hits
//! never ask whether an image is a view's.
//!
//! The painter draws with the device the view is given, which is the
//! renderer's own here. It may be another where the renderer draws through
//! something other than wgpu, so a painter keeps no device of its own
//! across frames without `Viewport::device_changed`.
use crate::{GpuError, GpuRenderer};
use std::cell::RefCell;

/// Draws a GPU view's frames.
pub trait Viewport: 'static {
    /// Draw a frame into `frame.target`, its every pixel (it holds the last
    /// frame or nothing): premultiplied alpha, `frame.size` device pixels.
    /// Record into `frame.encoder`, which is submitted before the frame
    /// showing it. Don't invalidate the view from here: animate it instead
    /// (`GpuView::animate`).
    fn render(&mut self, frame: &mut ViewportFrame<'_>);
    /// The device frames are drawn with changed (lost and made anew, or the
    /// view shown by a renderer on another): drop what was made with the old
    /// one. The next `render` follows.
    fn device_changed(&mut self) {}
}

/// What a [`Viewport`] draws a frame with.
#[non_exhaustive]
pub struct ViewportFrame<'a> {
    /// The device to draw with.
    pub device: &'a wgpu::Device,
    /// Its queue.
    pub queue: &'a wgpu::Queue,
    /// Commands to draw the frame with, submitted before the frame showing it.
    pub encoder: &'a mut wgpu::CommandEncoder,
    /// The frame's texture, `format` (unorm: what is written is what shows).
    pub target: &'a wgpu::TextureView,
    /// The same texture as sRGB (`format.add_srgb_suffix()`), for shading in
    /// linear light: what is written is encoded. `None` where the device
    /// can't view a texture in another format (GL).
    pub target_srgb: Option<&'a wgpu::TextureView>,
    /// `target`'s format.
    pub format: wgpu::TextureFormat,
    /// Its size in device pixels.
    pub size: (u32, u32),
    /// Device pixels a point.
    pub scale: f32,
    /// When it is drawn.
    pub time: std::time::Instant,
}

impl<'a> ViewportFrame<'a> {
    /// A frame to draw outside a renderer (a painter's own tests, a
    /// comparison with wgpu alone): no sRGB view, drawn now.
    pub fn new(
        device: &'a wgpu::Device,
        queue: &'a wgpu::Queue,
        encoder: &'a mut wgpu::CommandEncoder,
        target: &'a wgpu::TextureView,
        format: wgpu::TextureFormat,
        size: (u32, u32),
        scale: f32,
    ) -> Self {
        Self {
            device,
            queue,
            encoder,
            target,
            target_srgb: None,
            format,
            size,
            scale,
            time: std::time::Instant::now(),
        }
    }
}

/// A view's painter, and the device it last drew with.
struct Painter {
    viewport: Box<dyn Viewport>,
    device: Option<wgpu::Device>,
}

/// A GPU view drawn by `painter`: show it with `zgx::compose::gpu_view`.
pub fn view(painter: impl Viewport) -> zgx::gpu_view::GpuView {
    zgx::gpu_view::GpuView::new(RefCell::new(Painter {
        viewport: Box::new(painter),
        device: None,
    }))
}

/// A renderer's views: the frame each one's texture is cached under.
#[derive(Default)]
pub(crate) struct Views {
    /// By view and the box showing it: the image id its texture is cached
    /// under, and its size.
    shown: rustc_hash::FxHashMap<(u64, u64), Shown>,
}
struct Shown {
    image: u64,
    size: (u32, u32),
    /// Its texture's views for the painter: plain and sRGB.
    targets: (wgpu::TextureView, Option<wgpu::TextureView>),
}
impl Views {
    /// The image ids views' textures are cached under: forgotten, their next
    /// frames draw anew (at a new scale).
    pub(crate) fn forget(&mut self) -> impl Iterator<Item = u64> + '_ {
        self.shown.drain().map(|(_, shown)| shown.image)
    }
}

impl GpuRenderer {
    /// Draw `image`, a frame of a GPU view, into the view's texture (made, or
    /// remade at a new size) and cache it under the frame's id in place of
    /// the view's last frame.
    #[inline(never)]
    pub(crate) fn render_view(
        &mut self,
        image: &zgx::image::ImageData,
        frame: &zgx::gpu_view::GpuFrame,
    ) -> Result<(), GpuError> {
        let max = self.device.limits().max_texture_dimension_2d;
        let size = (
            ((frame.width * self.scale).ceil() as u32).clamp(1, max),
            ((frame.height * self.scale).ceil() as u32).clamp(1, max),
        );
        let views = self.views.get_or_insert_with(Default::default);
        // Its last frame's texture, if the same size: drawn over in place.
        let mut kept = None;
        let key = (frame.view, frame.shown_by);
        if let Some(last) = views.shown.remove(&key)
            && let Some((bind, bytes, Some(texture))) = self.images.remove(&last.image)
            && last.size == size
        {
            kept = Some((bind, bytes, texture, last.targets));
        }
        // Views gone: their entries (their textures went with their frames).
        if views.shown.len() > 64 {
            let images = &self.images;
            views
                .shown
                .retain(|_, shown| images.contains_key(&shown.image));
        }
        let (bind, bytes, texture, targets) = match kept {
            Some(kept) => kept,
            None => {
                let bytes = size.0 as usize * size.1 as usize * 4;
                if bytes + self.images.values().map(|entry| entry.1).sum::<usize>()
                    > 64 * 1024 * 1024
                {
                    return Err(GpuError(
                        "visible image and GPU view textures exceed their budget".into(),
                    ));
                }
                let texture = self.view_texture(key, size);
                let target = texture.create_view(&Default::default());
                let bind = self.texture_bind(&target, "GPU view");
                let srgb = self.view_srgb().then(|| {
                    texture.create_view(&wgpu::TextureViewDescriptor {
                        format: Some(crate::FORMAT.add_srgb_suffix()),
                        ..Default::default()
                    })
                });
                (bind, bytes, texture, (target, srgb))
            }
        };
        if let Some(painter) = zgx::gpu_view::painter(frame.view) {
            let painter = painter.downcast_ref::<RefCell<Painter>>().ok_or_else(|| {
                GpuError("a GPU view's painter isn't a viewport::Viewport".into())
            })?;
            let mut painter = painter.borrow_mut();
            let painter = &mut *painter;
            let (device, queue) = self.view_device();
            if painter.device.as_ref() != Some(device) {
                if painter.device.is_some() {
                    painter.viewport.device_changed();
                }
                painter.device = Some(device.clone());
            }
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("GPU view"),
            });
            painter.viewport.render(&mut ViewportFrame {
                device,
                queue,
                encoder: &mut encoder,
                target: &targets.0,
                target_srgb: targets.1.as_ref(),
                format: crate::FORMAT,
                size,
                scale: self.scale,
                time: std::time::Instant::now(),
            });
            // Ahead of the frame sampling it, which is recorded after this
            // and submitted (or committed, drawn with Metal directly) later.
            queue.submit([encoder.finish()]);
            self.after_view_submit(key);
        }
        self.images.insert(image.id(), (bind, bytes, Some(texture)));
        self.views.as_mut().unwrap().shown.insert(
            key,
            Shown {
                image: image.id(),
                size,
                targets,
            },
        );
        Ok(())
    }
    /// The device and queue views draw with: the renderer's own, as it
    /// draws with wgpu.
    fn view_device(&self) -> (&wgpu::Device, &wgpu::Queue) {
        (&self.device, &self.queue)
    }
    /// After a view's frame is submitted (`key`: the view and the box
    /// showing it), before the frame showing it: nothing to do where the
    /// renderer draws on the same queue.
    fn after_view_submit(&self, _key: (u64, u64)) {}
    /// The texture of a view's box (`key`: the view and the box): drawn to
    /// with the views' device, sampled by the renderer, and viewable as
    /// sRGB where the device allows.
    fn view_texture(&self, _key: (u64, u64), (width, height): (u32, u32)) -> wgpu::Texture {
        let srgb = [crate::FORMAT.add_srgb_suffix()];
        self.view_device()
            .0
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("GPU view"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: crate::FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::COPY_DST,
                view_formats: if self.view_srgb() { &srgb } else { &[] },
            })
    }
    /// Whether textures can be viewed in their sRGB twin format (not on GL).
    fn view_srgb(&self) -> bool {
        self.context
            .inner
            .adapter
            .get_downlevel_capabilities()
            .flags
            .contains(wgpu::DownlevelFlags::VIEW_FORMATS)
    }
}
