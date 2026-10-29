//! `fade_edges` as one mask over what a node holds, where it fades.
//!
//! A fade mask scales each item's alpha where it is drawn. Over a single
//! item that is the mask, but stacked items fade each on its own: dark art
//! on a light card both fade, and the card shows through the art, a light
//! veil where the band should go toward transparent. CSS's `mask-image`
//! fades the element's flattened pixels once. So does this, only in the
//! bands: the items under one mask draw directly where the mask is 1 (its
//! box less its bands, where it changes nothing), and what reaches a band
//! draws unfaded into a texture, composited once through the mask. Its
//! cost is the bands' area, whatever they hold.
//!
//! A run's bands are a phase of the frame (`BandPhase`): their items go
//! through `render_flat`'s loop first, into the frame's own quads, and draw
//! in a pass of the frame's own (its command buffer with Metal, its encoder
//! with wgpu) into one texture over them, before the window's pass. That
//! texture composites as a cached layer does, keyed by the id of an item of
//! the frame that is no layer (so nothing the scene says about that id
//! makes it composite otherwise), held until the next frame has drawn any
//! frame deferred before it.
use crate::{Draw, GpuRenderer, LayerCache, Scissor};
use std::rc::Rc;
use zgx::scene::{FadeMask, NodeId, NodeKind, PaintExtras, PaintItem, Rect, Scene};

/// A band texture's `LayerCache::revision`: no layer's.
const BAND: u64 = u64::MAX;
/// How far inside a device pixel's edge a clip's edge sits, so that
/// `scissor` (which floors where it starts and ceils where it ends) puts
/// the interior's and a band's shared edge on the same pixel boundary.
const BIAS: f32 = 0.25;

/// A run's bands as a phase of the frame (see the module's comment): its
/// items, in the band texture's points, drawn in a pass of their own.
pub(crate) struct BandPhase<'a> {
    pub(crate) items: Vec<PaintItem<'a>>,
    /// `layer_origin` while its items are drawn: the texture's corner.
    pub(crate) origin: (f32, f32),
    /// The texture's whole size, in points (the pass's viewport).
    pub(crate) viewport: Rect,
    /// Its bands, in its points: what the loop draws, and the pass's regions.
    pub(crate) damage: Vec<Rect>,
    pub(crate) regions: Vec<(Rect, Scissor)>,
    pub(crate) view: wgpu::TextureView,
    /// Its texture, for Metal directly (`render_direct`).
    #[cfg(target_os = "macos")]
    pub(crate) texture: wgpu::Texture,
    /// The loop's draws for it (`render_flat`).
    pub(crate) draws: Vec<Draw>,
}

/// A box in device pixels: left, top, right, bottom.
#[derive(Clone, Copy, PartialEq)]
struct Pixels([f32; 4]);
impl Pixels {
    fn empty(self) -> bool {
        self.0[2] <= self.0[0] || self.0[3] <= self.0[1]
    }
    fn meet(self, other: Self) -> Self {
        let [a, b] = [self.0, other.0];
        Self([
            a[0].max(b[0]),
            a[1].max(b[1]),
            a[2].min(b[2]),
            a[3].min(b[3]),
        ])
    }
    /// Covering `rect` (in points at `scale`).
    fn around(rect: Rect, scale: f32) -> Self {
        Self([
            (rect.x * scale).floor(),
            (rect.y * scale).floor(),
            ((rect.x + rect.width) * scale).ceil(),
            ((rect.y + rect.height) * scale).ceil(),
        ])
    }
    /// As a clip: its edges just inside its pixels' (see `BIAS`).
    fn clip(self, scale: f32) -> Rect {
        let [l, t, r, b] = self.0;
        Rect::new(
            (l + BIAS) / scale,
            (t + BIAS) / scale,
            (r - l - 2. * BIAS) / scale,
            (b - t - 2. * BIAS) / scale,
        )
    }
    /// As a texture's box: on its pixels exactly.
    fn rect(self, scale: f32) -> Rect {
        let [l, t, r, b] = self.0;
        Rect::new(l / scale, t / scale, (r - l) / scale, (b - t) / scale)
    }
}

/// `extras` without a fade mask (shared, for items sharing them).
fn unmasked(
    extras: &Option<Rc<PaintExtras>>,
    last: &mut Option<(*const PaintExtras, Option<Rc<PaintExtras>>)>,
) -> Option<Rc<PaintExtras>> {
    let extras = extras.as_ref()?;
    if let Some((from, to)) = last
        && std::ptr::eq(*from, &**extras)
    {
        return to.clone();
    }
    let mut plain = PaintExtras::clone(extras);
    plain.mask = None;
    let plain = (plain != PaintExtras::default()).then(|| Rc::new(plain));
    *last = Some((Rc::as_ptr(extras), plain.clone()));
    plain
}

impl GpuRenderer {
    /// Fade every item where it is drawn, as before fade bands drew as
    /// one: to compare the two (tests).
    #[doc(hidden)]
    pub fn set_fade_bands(&mut self, enabled: bool) {
        self.bands_off = !enabled;
    }
    /// Whether this renderer draws fade bands as one: not with Direct3D 11,
    /// which draws no layers (its fades go item by item, as they did).
    pub(crate) fn band_capable(&self) -> bool {
        if self.bands_off {
            return false;
        }
        #[cfg(d3d11)]
        if self.d3d11.is_some() {
            return false;
        }
        true
    }
    /// Band textures back into the pool, once the frames drawing them have
    /// been encoded (after `flush_pending`).
    pub(crate) fn reclaim_bands(&mut self) {
        for (key, view) in std::mem::take(&mut self.band_keys) {
            match self.layers.remove(&key) {
                Some(cache) if cache.revision == BAND => {
                    self.band_pool.push((cache.texture, cache.bind, view));
                }
                Some(cache) => {
                    self.layers.insert(key, cache);
                }
                None => {}
            }
        }
    }
    /// `items` with each run under one fade mask drawn as one: directly
    /// inside its bands, and in the bands it reaches through a texture
    /// composited after the run, drawn by the phases returned (the module's
    /// comment). In this target's points, drawing `damage`. Unchanged where
    /// nothing fades.
    pub(crate) fn band<'a>(
        &mut self,
        scene: &Scene,
        items: Vec<PaintItem<'a>>,
        damage: &[Rect],
        viewport: Rect,
    ) -> (Vec<PaintItem<'a>>, Vec<BandPhase<'a>>) {
        let scale = self.scale;
        let target = Pixels::around(viewport, scale);
        let Some(reach) = damage
            .iter()
            .copied()
            .reduce(|a, b| a.union(b))
            .map(|d| Pixels::around(d, scale).meet(target))
            .filter(|d| !d.empty())
        else {
            return (items, Vec::new());
        };
        // Most frames with a fade have none to draw as one (rows apart):
        // they go as they came, nothing moved or made.
        if !self.needs_bands(scene, &items, damage, viewport) {
            return (items, Vec::new());
        }
        let mut out: Vec<PaintItem<'a>> = Vec::with_capacity(items.len() + 4);
        let mut phases = Vec::new();
        let mut items = items.into_iter().peekable();
        while let Some(item) = items.next() {
            let Some(mask) = item.mask() else {
                out.push(item);
                continue;
            };
            let mut run = vec![item];
            while let Some(item) = items.next_if(|item| item.mask() == Some(mask)) {
                run.push(item);
            }
            // An id to key its texture: an item of it that no layer, cached
            // or not, has (nor another band).
            let key = run
                .iter()
                .find(|item| {
                    !item.isolated && item.id != scene.root() && !self.layers.contains_key(&item.id)
                })
                .map(|item| item.id);
            match (self.band_strips(scene, &run, mask, reach, target), key) {
                (Some(bands), Some(key)) => {
                    phases.push(self.band_run(run, mask, bands, key, target, &mut out));
                }
                // None stacking in a band (rows apart), or something there
                // it can't draw apart (a backdrop filter, a blend): faded
                // item by item, as it was.
                _ => out.extend(run),
            }
        }
        (out, phases)
    }
    /// Whether a walk's `items` (in this target's points, drawing
    /// `damage`) hold a run whose bands need drawing as one: a frame kept
    /// to patch would fade them item by item.
    pub(crate) fn needs_bands(
        &self,
        scene: &Scene,
        items: &[PaintItem<'_>],
        damage: &[Rect],
        viewport: Rect,
    ) -> bool {
        let scale = self.scale;
        let target = Pixels::around(viewport, scale);
        let Some(reach) = damage
            .iter()
            .copied()
            .reduce(|a, b| a.union(b))
            .map(|d| Pixels::around(d, scale).meet(target))
            .filter(|d| !d.empty())
        else {
            return false;
        };
        let mut at = 0;
        while at < items.len() {
            let Some(mask) = items[at].mask() else {
                at += 1;
                continue;
            };
            let end = items[at..]
                .iter()
                .position(|item| item.mask() != Some(mask))
                .map_or(items.len(), |n| at + n);
            if self
                .band_strips(scene, &items[at..end], mask, reach, target)
                .is_some()
            {
                return true;
            }
            at = end;
        }
        false
    }
    /// A run's interior and bands (in device pixels) where it needs its
    /// bands drawn as one: `None` where it can't be (a backdrop filter or
    /// blend in one), or where nothing in a band overlaps (item by item is
    /// exact there, a list of rows: drawn as it was).
    fn band_strips(
        &self,
        scene: &Scene,
        run: &[PaintItem<'_>],
        (rect, [top, right, bottom, left]): FadeMask,
        reach: Pixels,
        target: Pixels,
    ) -> Option<(Pixels, Vec<Pixels>)> {
        let scale = self.scale;
        let px = |v: f32| (v * scale).round();
        let [vl, vt, vr, vb] = target.0;
        // Where the mask is 0 (outside its box on a faded side) nothing
        // shows; within its box less its bands it is 1.
        let outer = Pixels([
            if left > 0. { px(rect.x) } else { vl },
            if top > 0. { px(rect.y) } else { vt },
            if right > 0. {
                px(rect.x + rect.width)
            } else {
                vr
            },
            if bottom > 0. {
                px(rect.y + rect.height)
            } else {
                vb
            },
        ])
        .meet(target);
        let [ol, ot, or, ob] = outer.0;
        let inner = Pixels([
            if left > 0. {
                px(rect.x + left).clamp(ol, or)
            } else {
                ol
            },
            if top > 0. {
                px(rect.y + top).clamp(ot, ob)
            } else {
                ot
            },
            if right > 0. {
                px(rect.x + rect.width - right).clamp(ol, or)
            } else {
                or
            },
            if bottom > 0. {
                px(rect.y + rect.height - bottom).clamp(ot, ob)
            } else {
                ob
            },
        ]);
        let [il, it, ir, ib] = inner.0;
        let strips: Vec<Pixels> = if inner.empty() {
            // Bands meeting: all of it fades.
            vec![outer]
        } else {
            [
                Pixels([ol, ot, or, it]),
                Pixels([ol, ib, or, ob]),
                Pixels([ol, it, il, ib]),
                Pixels([ir, it, or, ib]),
            ]
            .into_iter()
            .filter(|strip| !strip.empty())
            .collect()
        };
        // Of its bands, what this frame draws.
        let strips: Vec<Pixels> = strips
            .into_iter()
            .map(|strip| strip.meet(reach))
            .filter(|strip| !strip.empty())
            .collect();
        // What each paints that reaches a band: two sharing a pixel there
        // stack, faded item by item. Most boxes (a row, a container) paint
        // nothing of their own. One pass, to the first two that stack.
        let painted = |item: &PaintItem<'_>| -> Option<Pixels> {
            if item.effects.opacity <= 0. {
                return None;
            }
            let area = match item.kind {
                _ if item.isolated => item.clip.unwrap_or(item.bounds),
                NodeKind::Container(_) => return None,
                NodeKind::Quad(quad) | NodeKind::Panel { quad, .. } => {
                    if quad.fill.3 == 0
                        && quad.border_width <= 0.
                        && quad.shadow.is_none()
                        && quad.outline.is_none()
                        && quad.decoration.is_none()
                    {
                        return None;
                    }
                    quad.paint_bounds(item.bounds)
                }
                NodeKind::Rect(color) if color.3 == 0 => return None,
                NodeKind::Image(image) => image.paint_bounds(item.bounds),
                NodeKind::Svg(svg) => svg.paint_bounds(item.bounds),
                _ => item.bounds,
            };
            let area = match item.clip {
                Some(clip) => area.intersection(clip)?,
                None => area,
            };
            let area = Pixels::around(area, scale);
            strips
                .iter()
                .any(|strip| !strip.meet(area).empty())
                .then_some(area)
        };
        let mut painting: Vec<Pixels> = Vec::new();
        let stacks = run.iter().filter_map(painted).any(|area| {
            let stacks = painting.iter().any(|other| {
                let both = area.meet(*other);
                !both.empty() && strips.iter().any(|strip| !strip.meet(both).empty())
            });
            painting.push(area);
            stacks
        });
        if !stacks {
            return None;
        }
        // What the run can paint, its bands cut to it.
        let reaches = |item: &PaintItem<'_>| {
            item.clip
                .map_or(target, |clip| Pixels::around(clip, scale).meet(target))
        };
        let content = run
            .iter()
            .filter(|item| item.effects.opacity > 0.)
            .map(reaches)
            .reduce(|a, b| {
                Pixels([
                    a.0[0].min(b.0[0]),
                    a.0[1].min(b.0[1]),
                    a.0[2].max(b.0[2]),
                    a.0[3].max(b.0[3]),
                ])
            })?;
        let strips: Vec<Pixels> = strips
            .into_iter()
            .map(|strip| strip.meet(content))
            .filter(|strip| !strip.empty())
            .collect();
        // Drawn apart in a band: no backdrop filter (it samples what is
        // under it in the window) and no layer composited otherwise than
        // plainly.
        let apart = |item: &PaintItem<'_>| {
            item.effects.blur_radius <= 0.
                && (!item.isolated
                    || item.effects.filter_blur <= 0.
                        && scene.mask(item.id).is_none()
                        && scene.layer_shader(item.id).is_none()
                        && scene.blend(item.id) == zgx::scene::BlendMode::Normal)
        };
        let crossing = |item: &PaintItem<'_>| {
            item.effects.opacity > 0.
                && strips
                    .iter()
                    .any(|strip| !strip.meet(reaches(item)).empty())
        };
        if run
            .iter()
            .filter(|item| crossing(item))
            .any(|item| !apart(item))
        {
            return None;
        }
        Some((inner, strips))
    }
    /// One run under `mask` into `out`, its interior and `bands`
    /// (`band_strips`), and its bands' phase, its texture keyed by `key`.
    fn band_run<'a>(
        &mut self,
        run: Vec<PaintItem<'a>>,
        mask: FadeMask,
        (inner, strips): (Pixels, Vec<Pixels>),
        key: NodeId,
        target: Pixels,
        out: &mut Vec<PaintItem<'a>>,
    ) -> BandPhase<'a> {
        let scale = self.scale;
        let (rect, [top, right, bottom, left]) = mask;
        let crossing = |item: &PaintItem<'_>| {
            let reaches = item
                .clip
                .map_or(target, |clip| Pixels::around(clip, scale).meet(target));
            item.effects.opacity > 0. && strips.iter().any(|strip| !strip.meet(reaches).empty())
        };
        // Its bands: what reaches each, unfaded, into one texture over
        // them all, texel for pixel.
        let over = strips
            .iter()
            .copied()
            .reduce(|a, b| {
                Pixels([
                    a.0[0].min(b.0[0]),
                    a.0[1].min(b.0[1]),
                    a.0[2].max(b.0[2]),
                    a.0[3].max(b.0[3]),
                ])
            })
            .expect("a band");
        let bounds = over.rect(scale);
        // Each once, cut to the box over the bands: the pass's regions (the
        // bands) cut it to them.
        let within = over.clip(scale);
        let mut last = None;
        // What its composites name (they draw from the texture alone).
        let (kind, font, text_options) = (run[0].kind, run[0].font, run[0].text_options);
        let mut items: Vec<PaintItem<'a>> = run
            .iter()
            .filter(|item| crossing(item))
            .filter_map(|item| {
                let clip = item.clip.unwrap_or(within).intersection(within)?;
                let mut item = item.clone();
                item.clip = Some(clip);
                item.extras = unmasked(&item.extras, &mut last);
                Some(item)
            })
            .collect();
        zgx::scene::translate_items(&mut items, -bounds.x, -bounds.y);
        // The interior: drawn as it is, its mask (1 there) dropped, so it
        // takes the plainer shaders (which pay for its bands' pass, GPU).
        if !inner.empty() {
            let interior = inner.clip(scale);
            let mut last = None;
            out.extend(run.into_iter().filter_map(|mut item| {
                item.clip = Some(item.clip.unwrap_or(interior).intersection(interior)?);
                item.extras = unmasked(&item.extras, &mut last);
                Some(item)
            }));
        }
        let (width, height) = (
            (over.0[2] - over.0[0]) as u32,
            (over.0[3] - over.0[1]) as u32,
        );
        let found = self
            .band_pool
            .iter()
            .position(|(texture, ..)| texture.width() >= width && texture.height() >= height);
        let (texture, bind, view) = match found {
            Some(at) => self.band_pool.swap_remove(at),
            None => {
                let texture = crate::texture(
                    &self.device,
                    width.next_multiple_of(64),
                    height.next_multiple_of(64),
                    "fade band",
                );
                let view = texture.create_view(&Default::default());
                let bind = self.texture_bind(&view, "fade band");
                (texture, bind, view)
            }
        };
        let (ox, oy) = self.layer_origin;
        let world = Rect::new(bounds.x + ox, bounds.y + oy, bounds.width, bounds.height);
        self.layers.insert(
            key,
            LayerCache {
                texture: texture.clone(),
                bind,
                bounds: world,
                content: world,
                revision: BAND,
                root_origin: (world.x, world.y),
                bytes: texture.width() as usize * texture.height() as usize * 4,
                uv: [
                    width as f32 / texture.width() as f32,
                    height as f32 / texture.height() as f32,
                ],
                step: 0,
                factor: 1f32.to_bits(),
                exact: false,
                transformed: false,
                used: self.layer_frame,
                parked: false,
                watched: None,
            },
        );
        self.band_keys.push((key, view.clone()));
        // Its pass: the whole texture its viewport, its bands its regions.
        let viewport = Rect::new(
            0.,
            0.,
            texture.width() as f32 / scale,
            texture.height() as f32 / scale,
        );
        let damage: Vec<Rect> = strips
            .iter()
            .map(|strip| {
                let r = strip.rect(scale);
                Rect::new(r.x - bounds.x, r.y - bounds.y, r.width, r.height)
            })
            .collect();
        let regions = damage
            .iter()
            .filter_map(|rect| {
                crate::scissor(*rect, scale, texture.width(), texture.height()).map(|s| (*rect, s))
            })
            .collect();
        // Each band composited after the run, through the mask once.
        let extras = Some(Rc::new(PaintExtras {
            mask: Some((rect, [top, right, bottom, left])),
            ..Default::default()
        }));
        for strip in strips {
            out.push(PaintItem {
                isolated: true,
                id: key,
                bounds,
                kind,
                font,
                text_options,
                effects: zgx::scene::Effects::default(),
                clip: Some(strip.clip(scale)),
                border_pass: false,
                extras: extras.clone(),
            });
        }
        BandPhase {
            items,
            origin: (world.x, world.y),
            viewport,
            damage,
            regions,
            view,
            #[cfg(target_os = "macos")]
            texture,
            draws: Vec::new(),
        }
    }
    /// `band`'s pass with wgpu, into `encoder` before the frame's: its
    /// texture cleared, its draws in its regions. `regions`: where the
    /// frame's clip regions lie in `vertices`.
    pub(crate) fn encode_band(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        vertices: &wgpu::Buffer,
        regions: Option<(u64, u64)>,
        points: Option<&wgpu::BindGroup>,
        band: &BandPhase<'_>,
        stats: &mut crate::GpuStats,
    ) {
        let (uniform, plain) = self.viewport_uniform(band.viewport);
        let viewport = match regions {
            Some((offset, size)) => self.viewport_group(
                &uniform,
                wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: vertices,
                    offset,
                    size: wgpu::BufferSize::new(size),
                }),
            ),
            None => plain,
        };
        stats.render_passes += 1;
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("fade band"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &band.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_vertex_buffer(0, vertices.slice(..));
        pass.set_bind_group(0, &self.bind, &[]);
        pass.set_bind_group(1, &viewport, &[]);
        if let Some(points) = points {
            pass.set_bind_group(2, points, &[]);
        }
        let quads = &self.context.inner.quads;
        pass.set_pipeline(quads.get(crate::Shading::Full));
        for (region, (x, y, w, h)) in &band.regions {
            pass.set_scissor_rect(*x, *y, *w, *h);
            self.encode_draws(
                &mut pass,
                &|shading| {
                    self.fill_pipeline(shading)
                        .unwrap_or_else(|| quads.get(shading))
                },
                *region,
                &band.draws,
                0,
                stats,
            );
        }
    }
    /// The viewport uniform of a target `viewport` in size, and its bind
    /// group with no regions (made if new), leaving the frame's as it is.
    pub(crate) fn viewport_uniform(&mut self, viewport: Rect) -> (wgpu::Buffer, wgpu::BindGroup) {
        let key = self.viewport_entry(viewport.width, viewport.height);
        self.viewports.get(&key).cloned().expect("made")
    }
}
