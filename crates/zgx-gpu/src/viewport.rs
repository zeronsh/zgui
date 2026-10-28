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
    /// Boxes shown on planes of their own (macOS), by view and box.
    #[cfg(target_os = "macos")]
    planes: rustc_hash::FxHashMap<(u64, u64), Planed>,
    /// Planes moved or shown this frame, presented in its transaction.
    #[cfg(target_os = "macos")]
    pub(crate) moved: Vec<crate::layer_present::PlaneFrame>,
    /// Planes to hide in this frame's transaction (their views composited),
    /// kept for when they show again.
    #[cfg(target_os = "macos")]
    pub(crate) hidden: Vec<(u64, u64)>,
    /// Planes whose boxes are gone, to drop in this frame's transaction.
    #[cfg(target_os = "macos")]
    pub(crate) gone: Vec<(u64, u64)>,
}
/// A box on a plane: its node, the frame it last drew, its box, and the
/// drawables' textures as wgpu's, by Metal texture (the pool's few).
#[cfg(target_os = "macos")]
struct Planed {
    node: zgx::scene::NodeId,
    image: u64,
    rect: zgx::scene::Rect,
    /// Its layer shows (not only primed: see `plane_item`).
    shown: bool,
    /// While its layer is hidden, about to show: a drawable drawn and held,
    /// presented as it shows (a hidden layer's presented drawables don't
    /// come back to its pool).
    primed: Option<crate::layer_present::PlaneFrame>,
    targets: Vec<(usize, wgpu::Texture, wgpu::TextureView)>,
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
    /// Whether a plane moves, shows, hides or goes with this frame: the
    /// window's own drawable then presents in its transaction too.
    pub(crate) fn planes_moved(&self) -> bool {
        #[cfg(target_os = "macos")]
        return !self.moved.is_empty() || !self.hidden.is_empty() || !self.gone.is_empty();
        #[cfg(not(target_os = "macos"))]
        false
    }
    /// Whether a box shows on a plane: the window then keeps no frame to
    /// patch (its box draws nothing there).
    pub(crate) fn has_planes(&self) -> bool {
        #[cfg(target_os = "macos")]
        return !self.planes.is_empty();
        #[cfg(not(target_os = "macos"))]
        false
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
        self.paint(key, size, &targets.0, targets.1.as_ref(), crate::FORMAT)?;
        // Composited: off its plane, if it was on one, in this frame's
        // transaction (the frame drawing it into the window shows with it).
        #[cfg(target_os = "macos")]
        if let Some(views) = self.views.as_mut()
            && views.planes.remove(&key).is_some()
        {
            views.hidden.push(key);
            zgx::gpu_view::set_planed(key.0, key.1, false);
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
    /// Have view `key.0`'s painter draw a frame of `size` into `target`, its
    /// work submitted before the frame showing it. Nothing if the view is
    /// gone.
    fn paint(
        &self,
        key: (u64, u64),
        size: (u32, u32),
        target: &wgpu::TextureView,
        target_srgb: Option<&wgpu::TextureView>,
        format: wgpu::TextureFormat,
    ) -> Result<(), GpuError> {
        let Some(painter) = zgx::gpu_view::painter(key.0) else {
            return Ok(());
        };
        let painter = painter
            .downcast_ref::<RefCell<Painter>>()
            .ok_or_else(|| GpuError("a GPU view's painter isn't a viewport::Viewport".into()))?;
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
            target,
            target_srgb,
            format,
            size,
            scale: self.scale,
            time: std::time::Instant::now(),
        });
        // Ahead of the frame sampling it (or its drawable's present), which
        // is recorded after this and submitted later on the same queue.
        queue.submit([encoder.finish()]);
        self.after_view_submit(key);
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

/// Planes: a box showing a GPU view whole and opaque gets a layer of its
/// own, its painter drawing into the layer's drawables, so neither the
/// renderer nor the window server composites it with the window's frame.
#[cfg(target_os = "macos")]
impl GpuRenderer {
    /// Draw `item`, a GPU view's frame, on a plane of its own if it may be
    /// (see `planeable`); whether it was, in which case the window draws
    /// nothing for it.
    #[inline(never)]
    pub(crate) fn plane_item(
        &mut self,
        scene: &zgx::scene::Scene,
        item: &zgx::scene::PaintItem,
        image: &zgx::image::ImageData,
    ) -> Result<bool, GpuError> {
        let Some(frame) = image.gpu_frame() else {
            return Ok(false);
        };
        if !self.planeable(scene, item, image, frame) {
            return Ok(false);
        }
        let key = (frame.view, frame.shown_by);
        let shown = self
            .views
            .as_ref()
            .is_some_and(|views| views.planes.get(&key).is_some_and(|planed| planed.shown));
        if shown {
            return self.draw_plane(item.id, image, frame, item.bounds, true);
        }
        // Taking its layer: this frame still draws it into the window, and
        // the layer, hidden, gets a newer frame after it; the next shows the
        // layer. A hidden layer still holds the frame it showed last, from
        // before it hid: shown in the same frame as its new one, it could
        // show that old one first, a frame going back.
        self.render_view(image, frame)?;
        self.draw_plane(item.id, image, frame, item.bounds, false)?;
        Ok(false)
    }
    /// Whether a frame may show on a plane: its painter promises it opaque,
    /// the window presents through layers of its own, and its box shows it
    /// whole and alone: drawn flat in the window (not inside a cached layer),
    /// untransformed, unclipped, unrounded, unfaded, nothing painted over it.
    fn planeable(
        &self,
        scene: &zgx::scene::Scene,
        item: &zgx::scene::PaintItem,
        image: &zgx::image::ImageData,
        frame: &zgx::gpu_view::GpuFrame,
    ) -> bool {
        frame.opaque
            && !self.in_layer
            && self
                .layer_present
                .as_ref()
                .is_some_and(|present| present.layer.is_some())
            && item.extras.is_none()
            && item.effects == zgx::scene::Effects::default()
            && image.transform() == zgx::affine::Affine::IDENTITY
            && item
                .clip
                .is_none_or(|clip| clip.intersection(item.bounds) == Some(item.bounds))
            && !painted_over(scene, item.id, item.bounds)
    }
    /// Draw `image`'s frame into its plane's next drawable, `rect` its box.
    /// `show`: its layer shows with this frame (else it stays hidden, the
    /// frame drawn for when it does).
    fn draw_plane(
        &mut self,
        node: zgx::scene::NodeId,
        image: &zgx::image::ImageData,
        frame: &zgx::gpu_view::GpuFrame,
        rect: zgx::scene::Rect,
        show: bool,
    ) -> Result<bool, GpuError> {
        let key = (frame.view, frame.shown_by);
        // Showing a primed layer: its held drawable presents as it shows, in
        // the frame's transaction, if its box is still where it was (moved
        // since, it is drawn again below, the held one going back unshown).
        if show
            && let Some(planed) = self
                .views
                .as_mut()
                .and_then(|views| views.planes.get_mut(&key))
            && let Some(mut primed) = planed.primed.take()
            && primed.rect() == planed_points(rect)
        {
            primed.moved = true;
            planed.shown = true;
            planed.node = node;
            planed.image = image.id();
            self.views.as_mut().unwrap().moved.push(primed);
            return Ok(true);
        }
        let max = self.device.limits().max_texture_dimension_2d;
        let size = (
            ((rect.width * self.scale).round() as u32).clamp(1, max),
            ((rect.height * self.scale).round() as u32).clamp(1, max),
        );
        // SAFETY: Only the raw device is read; the guard drops at once.
        let Some(device) = (unsafe { self.device.as_hal::<wgpu::hal::api::Metal>() }).map(|hal| {
            use objc2::Message;
            hal.raw_device().retain()
        }) else {
            return Ok(false);
        };
        let Some(layer) = self
            .layer_present
            .as_mut()
            .and_then(|present| present.layer.as_mut())
        else {
            return Ok(false);
        };
        let points = planed_points(rect);
        let Some(mut plane) = layer.plane_frame(key, &device, size, points) else {
            return Ok(false);
        };
        // The drawable's texture as wgpu's, made once per texture of the pool.
        use objc2_metal::MTLTexture;
        use objc2_quartz_core::CAMetalDrawable;
        let texture = plane.drawable.texture();
        let address = &*texture as *const _ as *const u8 as usize;
        let views = self.views.get_or_insert_with(Default::default);
        let planed = views.planes.entry(key).or_insert_with(|| Planed {
            node,
            image: 0,
            rect,
            shown: false,
            primed: None,
            targets: Vec::new(),
        });
        // Shown now (after a primed frame): with the window's frame.
        plane.moved = show && (plane.moved || !planed.shown);
        planed.shown |= show;
        planed.node = node;
        planed.image = image.id();
        planed.rect = rect;
        let target = match planed.targets.iter().position(|(at, ..)| *at == address) {
            Some(at) => planed.targets[at].2.clone(),
            None => {
                let (width, height) = (texture.width() as u32, texture.height() as u32);
                // SAFETY: Metal made this texture (the layer's drawable, on this
                // device) with this format and size, one mip level and layer.
                let raw = unsafe {
                    wgpu::hal::metal::Device::texture_from_raw(
                        texture.clone(),
                        wgpu::TextureFormat::Bgra8UnormSrgb,
                        objc2_metal::MTLTextureType::Type2D,
                        1,
                        1,
                        wgpu::hal::CopyExtent {
                            width,
                            height,
                            depth: 1,
                        },
                    )
                };
                // SAFETY: Made from this device's raw device as described; a
                // drawable is only ever drawn whole (the painter clears it).
                let wrapped = unsafe {
                    self.device
                        .create_texture_from_hal::<wgpu::hal::api::Metal>(
                            raw,
                            &wgpu::TextureDescriptor {
                                label: Some("GPU view plane"),
                                size: wgpu::Extent3d {
                                    width,
                                    height,
                                    depth_or_array_layers: 1,
                                },
                                mip_level_count: 1,
                                sample_count: 1,
                                dimension: wgpu::TextureDimension::D2,
                                format: wgpu::TextureFormat::Bgra8UnormSrgb,
                                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                                view_formats: &[],
                            },
                        )
                };
                let view = wrapped.create_view(&Default::default());
                // A new size or pool: the old textures are gone with it.
                if planed.targets.len() >= 3 {
                    planed.targets.remove(0);
                }
                planed.targets.push((address, wrapped, view.clone()));
                view
            }
        };
        zgx::gpu_view::set_planed(key.0, key.1, true);
        self.paint(
            key,
            size,
            &target,
            None,
            wgpu::TextureFormat::Bgra8UnormSrgb,
        )?;
        if !show {
            // Primed: held for when its layer shows.
            if let Some(planed) = self
                .views
                .as_mut()
                .and_then(|views| views.planes.get_mut(&key))
            {
                planed.primed = Some(plane);
            }
        } else if plane.moved {
            // In the frame's transaction, with the window's frame.
            self.views.as_mut().unwrap().moved.push(plane);
        } else {
            self.present_plane(plane);
        }
        Ok(show)
    }
    /// Present `plane` after the work on the queue (its painter's).
    fn present_plane(&mut self, plane: crate::layer_present::PlaneFrame) {
        // SAFETY: The raw queue is only retained; the guard drops at once.
        let queue = unsafe { self.queue.as_hal::<wgpu::hal::api::Metal>() }.map(|hal| {
            use objc2::Message;
            hal.as_raw().retain()
        });
        if let (Some(queue), Some(layer)) = (
            queue,
            self.layer_present
                .as_mut()
                .and_then(|present| present.layer.as_mut()),
        ) {
            layer.present_plane_now(plane, &queue);
        }
    }
    /// Draw the new frames of views on planes whose boxes did not change: a
    /// frame with nothing else to draw (no damage). Whether any was drawn.
    pub(crate) fn render_planes_now(
        &mut self,
        scene: &zgx::scene::Scene,
    ) -> Result<bool, GpuError> {
        let Some(views) = self.views.as_ref() else {
            return Ok(false);
        };
        let due: Vec<_> = views
            .planes
            .values()
            .filter_map(|planed| {
                if !scene.contains(planed.node) {
                    return None;
                }
                match scene.kind(planed.node) {
                    zgx::scene::NodeKind::Image(image) if image.id() != planed.image => {
                        Some((planed.node, image.clone(), planed.rect))
                    }
                    _ => None,
                }
            })
            .collect();
        let mut drawn = false;
        for (node, image, rect) in due {
            if let Some(frame) = image.gpu_frame() {
                drawn |= self.draw_plane(node, &image, frame, rect, true)?;
            }
        }
        // A plane showing with no frame of the window's to show with (none
        // drawn since its last): in a transaction of its own, now.
        if self
            .views
            .as_ref()
            .is_some_and(|views| views.planes_moved())
            && self
                .layer_present
                .as_ref()
                .is_none_or(|present| present.current.is_none())
        {
            self.apply_planes();
        }
        Ok(drawn)
    }
    /// After a frame's render: the planes whose boxes are gone, to drop in
    /// its transaction (`apply_planes`).
    pub(crate) fn settle_planes(&mut self, scene: &zgx::scene::Scene) {
        let Some(views) = self.views.as_mut() else {
            return;
        };
        let mut gone = Vec::new();
        views.planes.retain(|key, planed| {
            let alive = scene.contains(planed.node);
            if !alive {
                gone.push(*key);
            }
            alive
        });
        for key in gone {
            zgx::gpu_view::set_planed(key.0, key.1, false);
            views.gone.push(key);
        }
    }
    /// In the frame's transaction: show the planes that moved with their
    /// frames, hide those composited again, drop those gone.
    pub(crate) fn apply_planes(&mut self) {
        // SAFETY: The raw queue is only retained; the guard drops at once.
        let queue = unsafe { self.queue.as_hal::<wgpu::hal::api::Metal>() }.map(|hal| {
            use objc2::Message;
            hal.as_raw().retain()
        });
        if let (Some(views), Some(layer)) = (
            self.views.as_deref_mut(),
            self.layer_present
                .as_mut()
                .and_then(|present| present.layer.as_mut()),
        ) {
            schedule_planes(views, queue.as_deref());
            crate::layer_present::transaction(|| apply_planes(views, layer, queue.as_deref()));
        }
    }
}
/// See `GpuRenderer::apply_planes`: on `layer`, in the caller's transaction.
#[cfg(target_os = "macos")]
pub(crate) fn apply_planes(
    views: &mut Views,
    layer: &mut crate::layer_present::Layer,
    queue: Option<&objc2::runtime::ProtocolObject<dyn objc2_metal::MTLCommandQueue>>,
) {
    if views.moved.is_empty() && views.hidden.is_empty() && views.gone.is_empty() {
        return;
    }
    for key in views.hidden.drain(..) {
        layer.hide_plane(key);
    }
    if !views.gone.is_empty() {
        let gone = std::mem::take(&mut views.gone);
        layer.retain_planes(|key| !gone.contains(&key));
    }
    let _ = queue;
    for plane in views.moved.drain(..) {
        layer.show_plane(plane);
    }
}
/// Before the frame's transaction: the work of planes moving with it (and
/// the frame's own, on the same queue) committed and scheduled, so their
/// presents in the transaction wait for nothing.
#[cfg(target_os = "macos")]
pub(crate) fn schedule_planes(
    views: &Views,
    queue: Option<&objc2::runtime::ProtocolObject<dyn objc2_metal::MTLCommandQueue>>,
) {
    if !views.moved.is_empty()
        && let Some(queue) = queue
    {
        crate::layer_present::Layer::schedule_plane(queue);
    }
}

/// `rect` as a plane's box: the host's points from the top left.
#[cfg(target_os = "macos")]
fn planed_points(rect: zgx::scene::Rect) -> (f64, f64, f64, f64) {
    (
        f64::from(rect.x),
        f64::from(rect.y),
        f64::from(rect.width),
        f64::from(rect.height),
    )
}

/// Whether anything paints over `node` within `rect`: an item after it in
/// paint order whose box meets `rect`. Walks only the subtrees that paint
/// within `rect` (as damage is walked), so its cost follows what lies near
/// the view, not the window's size; and only on frames that draw the view's
/// item, never on a frame of the view alone.
#[cfg(target_os = "macos")]
fn painted_over(
    scene: &zgx::scene::Scene,
    node: zgx::scene::NodeId,
    rect: zgx::scene::Rect,
) -> bool {
    let mut after = false;
    for item in scene.layer_items_within(None, Some(&[rect])) {
        if after {
            if item
                .bounds
                .intersection(rect)
                .is_some_and(|met| met.width > 0. && met.height > 0.)
            {
                return true;
            }
        } else if item.id == node {
            after = true;
        }
    }
    false
}

/// Elsewhere, no view shows on a plane of its own.
#[cfg(not(target_os = "macos"))]
impl GpuRenderer {
    pub(crate) fn plane_item(
        &mut self,
        _scene: &zgx::scene::Scene,
        _item: &zgx::scene::PaintItem,
        _image: &zgx::image::ImageData,
    ) -> Result<bool, GpuError> {
        Ok(false)
    }
    pub(crate) fn render_planes_now(
        &mut self,
        _scene: &zgx::scene::Scene,
    ) -> Result<bool, GpuError> {
        Ok(false)
    }
    pub(crate) fn settle_planes(&mut self, _scene: &zgx::scene::Scene) {}
    pub(crate) fn apply_planes(&mut self) {}
}
