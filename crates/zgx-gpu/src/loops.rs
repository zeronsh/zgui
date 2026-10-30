//! Looping animations played by Core Animation (`Scene::set_baked_loop`).
//!
//! Any Metal submission holds about 176 MB of driver memory for the next
//! 8 to 13 seconds (M5, macOS 26.4), and a spinner drawn tick by tick wakes
//! the app and the GPU for as long as it spins. A declared loop's frames are
//! known ahead: they are drawn once into a small sheet, and a layer over the
//! window's frames plays them, with no submission, no commit and no wake.
//!
//! The item leaves Metal's paint only where a layer over the frames shows
//! exactly what it would: a lone item, uncut by its clip and region (a
//! rounded clip or fade changing none of its pixels is fine), with nothing
//! painted over it, in the window's own frames. Anywhere else the loop is
//! refused, and goes on tick by tick from where it has reached. A new look
//! or scale draws the sheet again.
use crate::layer_present::{Buffer, LoopShown};
use crate::{GpuRenderer, Region};
use std::rc::Rc;
use zgx::scene::{BakedLoop, NodeId, NodeKind, PaintItem, Rect, Scene};

/// The most bytes a loop's sheet takes: past it, it stays on Metal rather
/// than trade driver memory for a large sheet.
const MOST_BYTES: u64 = 4 << 20;
/// The most bytes a window's sheets take together.
const MOST_TOTAL: u64 = 32 << 20;

/// A loop's frames, drawn.
pub(crate) struct LoopSheet {
    node: NodeId,
    baked: Rc<BakedLoop>,
    /// Shared by loops that look alike.
    buffer: Rc<Buffer>,
    /// What it was drawn from: its look and raster scale.
    kind: NodeKind,
    params: [[f32; 4]; 4],
    scale: f32,
    /// The item's size, in points.
    size: (f32, f32),
    /// One frame's size in pixels, and how far into it the item starts.
    cell: (u32, u32),
    fraction: (f32, f32),
    /// Where the sheet's cells start in the loop: a run of equal frames is
    /// one cell.
    starts: Vec<u32>,
    columns: u32,
    rows: u32,
    /// Its place now, in points, and the opacity of what holds it.
    frame: (f32, f32),
    opacity: f32,
    /// The bounds it was last seen at: a sheet is drawn again for a new
    /// fraction only once they hold still, not through a slide.
    seen: Rect,
}
/// What a layer shows over the window's frame: read back with it, so a
/// capture of the frame shows what the window does (`ShownFrame::read`).
pub(crate) enum OverFrame {
    /// A loop's sheet, its corner at this pixel of the frame.
    Loop {
        buffer: Rc<Buffer>,
        at: (i32, i32),
        cell: (u32, u32),
        columns: u32,
        starts: Vec<u32>,
        frames: u32,
        fps: f32,
        started: std::time::Instant,
        opacity: f32,
    },
    /// The caret: this box of pixels, premultiplied BGRA.
    Caret {
        rect: (i32, i32, i32, i32),
        color: [u8; 4],
    },
}
impl OverFrame {
    /// Draws itself over `bytes`, `size` pixels of the frame from `origin`,
    /// premultiplied BGRA.
    pub(crate) fn draw(&self, bytes: &mut [u8], origin: (i32, i32), size: (i32, i32)) {
        let (width, height) = size;
        let mut over = |x: i32, y: i32, source: [u8; 4], opacity: u32| {
            let (x, y) = (x - origin.0, y - origin.1);
            if x < 0 || y < 0 || x >= width || y >= height {
                return;
            }
            let at = (y * width + x) as usize * 4;
            let keep = 255 - source[3] as u32 * opacity / 255;
            for channel in 0..4 {
                bytes[at + channel] = (source[channel] as u32 * opacity / 255
                    + bytes[at + channel] as u32 * keep / 255)
                    .min(255) as u8;
            }
        };
        match self {
            Self::Caret { rect, color } => {
                for y in rect.1..rect.1 + rect.3 {
                    for x in rect.0..rect.0 + rect.2 {
                        over(x, y, *color, 255);
                    }
                }
            }
            Self::Loop {
                buffer,
                at,
                cell,
                columns,
                starts,
                frames,
                fps,
                started,
                opacity,
            } => {
                // The frame it shows now, on the loop's clock.
                let frame = (started.elapsed().as_secs_f64() * *fps as f64) as u64 % *frames as u64;
                let index = starts.partition_point(|start| *start as u64 <= frame) - 1;
                let (left, top) = (
                    (index as u32 % columns * cell.0) as usize,
                    (index as u32 / columns * cell.1) as usize,
                );
                let surface = objc2_core_foundation::CFRetained::as_ptr(&buffer.surface)
                    .as_ptr()
                    .cast::<std::ffi::c_void>()
                    .cast_const();
                let opacity = (*opacity * 255.).round() as u32;
                // SAFETY: a live surface (held) of four bytes a pixel, and
                // no longer drawn into: its frames were waited for in `shown`.
                unsafe {
                    crate::iosurface::with_pixels(surface, |pixels, _, _, stride| {
                        for y in 0..cell.1 as usize {
                            let row = &pixels[(top + y) * stride + left * 4..];
                            for x in 0..cell.0 as usize {
                                let source =
                                    [row[x * 4], row[x * 4 + 1], row[x * 4 + 2], row[x * 4 + 3]];
                                over(at.0 + x as i32, at.1 + y as i32, source, opacity);
                            }
                        }
                    });
                }
            }
        }
    }
}

impl LoopSheet {
    fn key(&self) -> u64 {
        self.node.number() as u64
    }
    fn bytes(&self) -> u64 {
        let (width, height) = self.cell;
        (width * self.columns) as u64 * (height * self.rows) as u64 * 4
            / Rc::strong_count(&self.buffer) as u64
    }
    fn shown(&self, scale: f32) -> LoopShown {
        let (columns, rows) = (self.columns as f64, self.rows as f64);
        let rects = (0..self.starts.len() as u32)
            .map(|cell| {
                [
                    (cell % self.columns) as f64 / columns,
                    (cell / self.columns) as f64 / rows,
                    1. / columns,
                    1. / rows,
                ]
            })
            .collect();
        let total = self.baked.frames.len() as f64;
        let times = self
            .starts
            .iter()
            .map(|start| *start as f64 / total)
            .chain([1.])
            .collect();
        let duration = total / self.baked.fps as f64;
        let (width, height) = self.cell;
        LoopShown {
            key: self.key(),
            surface: self.buffer.surface.clone(),
            rect: (
                self.frame.0 as f64,
                self.frame.1 as f64,
                (width as f32 / scale) as f64,
                (height as f32 / scale) as f64,
            ),
            opacity: self.opacity,
            rects,
            times,
            duration,
            phase: self.baked.started.elapsed().as_secs_f64() % duration,
            frame: 1. / self.baked.fps as f64,
        }
    }
}

impl GpuRenderer {
    /// What layers show over the window's frames now (`OverFrame`). Empty,
    /// and nothing made, for a window with neither.
    pub(crate) fn over_frames(&self) -> Vec<OverFrame> {
        let scale = self.scale;
        let mut over: Vec<OverFrame> = self
            .loop_sheets
            .iter()
            .map(|sheet| OverFrame::Loop {
                buffer: sheet.buffer.clone(),
                at: (
                    (sheet.frame.0 * scale).round() as i32,
                    (sheet.frame.1 * scale).round() as i32,
                ),
                cell: sheet.cell,
                columns: sheet.columns,
                starts: sheet.starts.clone(),
                frames: sheet.baked.frames.len() as u32,
                fps: sheet.baked.fps,
                started: sheet.baked.started,
                opacity: sheet.opacity,
            })
            .collect();
        if let Some(caret) = self
            .layer_present
            .as_ref()
            .and_then(|present| present.caret)
            .filter(|caret| caret.shown)
        {
            let (x, y, width, height) = caret.rect;
            let scale = scale as f64;
            let [r, g, b, a] = caret.color;
            let byte = |value: f64| (value * a * 255.).round() as u8;
            over.push(OverFrame::Caret {
                rect: (
                    (x * scale).round() as i32,
                    (y * scale).round() as i32,
                    (width * scale).round() as i32,
                    (height * scale).round() as i32,
                ),
                color: [byte(b), byte(g), byte(r), (a * 255.).round() as u8],
            });
        }
        over
    }
    /// Test hook: the frame shown, without what layers show over it.
    #[doc(hidden)]
    pub fn debug_bare_pixels(&self) -> Option<(u32, u32, Vec<u8>)> {
        let mut frame = self.shown_frame()?;
        frame.over.clear();
        frame.read(0, 0, u32::MAX, u32::MAX)
    }
    /// Test hook: the surface of each loop's sheet.
    #[doc(hidden)]
    pub fn debug_loop_sheets(&self) -> Vec<u32> {
        self.loop_sheets
            .iter()
            .map(|s| s.buffer.surface.id())
            .collect()
    }
    /// Test hook: whether every sheet is tagged as the window's surfaces are.
    #[doc(hidden)]
    pub fn debug_loops_tagged_as_frames(&self) -> bool {
        self.loop_sheets.iter().all(|s| s.buffer.tagged_srgb())
    }
    /// Whether this renderer's frames can carry loops' layers: the window's
    /// own, presented on the plain layer.
    fn plays_loops(&self) -> bool {
        self.plane.is_none()
            && self
                .layer_present
                .as_ref()
                .is_some_and(|present| present.layer.is_some())
    }
    /// `items` less the looping ones a layer plays, those layers placed,
    /// their sheets drawn if new. A frame's `items` may leave out what its
    /// damage doesn't reach: such a loop's layer stays as it is.
    pub(crate) fn place_loops<'a>(
        &mut self,
        scene: &Scene,
        mut items: Vec<PaintItem<'a>>,
    ) -> Vec<PaintItem<'a>> {
        let plays = self.plays_loops();
        scene.set_plays_loops(plays && zgx::switches::BAKED_LOOPS.on());
        let baked = scene.baked_loops();
        if baked.is_empty() && self.loop_sheets.is_empty() {
            return items;
        }
        // (Whatever this frame's damage reaches: a child added damages only
        // itself, and would draw under the layer.)
        for (node, loop_) in baked {
            if !scene.children(*node).is_empty() {
                loop_.refuse();
            }
        }
        let scale = self.scale;
        let mut placed = Vec::new();
        items.retain(|item| {
            let Some((_, loop_)) = baked.iter().find(|(node, _)| *node == item.id) else {
                return true;
            };
            if plays && loop_shows(scene, item, scale) {
                placed.push((item.clone(), loop_.clone()));
                return false;
            }
            loop_.refuse();
            true
        });
        self.loop_sheets.retain(|sheet| {
            baked.iter().any(|(node, loop_)| {
                *node == sheet.node && Rc::ptr_eq(loop_, &sheet.baked) && !loop_.refused()
            })
        });
        let mut changed = Vec::new();
        for (item, loop_) in placed {
            let bounds = item.bounds;
            let (x, y) = (bounds.x * scale, bounds.y * scale);
            let fraction = (x - x.floor(), y - y.floor());
            let own = scene.effects(item.id).opacity;
            let opacity = if own > 0. {
                item.effects.opacity / own
            } else {
                1.
            };
            let key = item.id.number() as u64;
            let params = scene.shader_uniforms(item.id).params;
            let at = self.loop_sheets.iter().position(|s| s.node == item.id);
            let stale = at.is_some_and(|at| {
                let sheet = &self.loop_sheets[at];
                sheet.scale != scale
                    || sheet.params != params
                    || sheet.kind != *item.kind
                    || (sheet.fraction != fraction && sheet.seen == bounds)
            });
            if at.is_none() || stale {
                self.loop_sheets.retain(|sheet| sheet.node != item.id);
                let Some(sheet) = self.bake_loop(scene, &item, &loop_, fraction) else {
                    loop_.refuse();
                    continue;
                };
                self.loop_sheets.push(sheet);
                changed.push(key);
            }
            let Some(sheet) = self.loop_sheets.iter_mut().find(|s| s.key() == key) else {
                continue;
            };
            sheet.seen = bounds;
            let frame = (
                (x - sheet.fraction.0) / scale,
                (y - sheet.fraction.1) / scale,
            );
            if sheet.frame != frame || sheet.opacity != opacity {
                sheet.frame = frame;
                sheet.opacity = opacity;
                if !changed.contains(&key) {
                    changed.push(key);
                }
            }
        }
        let alive = self.loop_sheets.iter().map(LoopSheet::key).collect();
        let shown = self
            .loop_sheets
            .iter()
            .filter(|sheet| changed.contains(&sheet.key()))
            .map(|sheet| sheet.shown(scale))
            .collect();
        if let Some(layer) = self
            .layer_present
            .as_mut()
            .and_then(|present| present.layer.as_mut())
        {
            layer.set_loops(alive, shown);
        }
        items
    }
    /// Loops among a layer's `items` show frozen there: they go on tick by
    /// tick (`loop_sheets` leaves them when next placed).
    pub(crate) fn refuse_layered_loops(&self, scene: &Scene, items: &[PaintItem<'_>]) {
        let baked = scene.baked_loops();
        if baked.is_empty() || self.loop_time.is_some() {
            return;
        }
        for item in items {
            if let Some((_, loop_)) = baked.iter().find(|(node, _)| *node == item.id) {
                loop_.refuse();
            }
        }
    }
    /// Draws one period of `item`'s `loop_` into a new sheet; `None` for a
    /// loop past the byte limits.
    fn bake_loop(
        &mut self,
        scene: &Scene,
        item: &PaintItem<'_>,
        loop_: &Rc<BakedLoop>,
        fraction: (f32, f32),
    ) -> Option<LoopSheet> {
        let scale = self.scale;
        // Another loop that looks the same, frame for frame: its sheet.
        let params = scene.shader_uniforms(item.id).params;
        let twin = self.loop_sheets.iter().find(|sheet| {
            sheet.scale == scale
                && sheet.fraction == fraction
                && sheet.params == params
                && sheet.kind == *item.kind
                && sheet.size == (item.bounds.width, item.bounds.height)
                && sheet.baked.fps == loop_.fps
                && sheet.baked.frames == loop_.frames
        });
        if let Some(twin) = twin {
            return Some(LoopSheet {
                node: item.id,
                baked: loop_.clone(),
                buffer: twin.buffer.clone(),
                kind: twin.kind.clone(),
                params,
                scale,
                size: twin.size,
                cell: twin.cell,
                fraction,
                starts: twin.starts.clone(),
                columns: twin.columns,
                rows: twin.rows,
                frame: (f32::NAN, f32::NAN),
                opacity: 1.,
                seen: Rect::default(),
            });
        }
        let cell = (
            (item.bounds.width * scale + fraction.0).ceil() as u32 + 1,
            (item.bounds.height * scale + fraction.1).ceil() as u32 + 1,
        );
        let starts: Vec<u32> = (0..loop_.frames.len())
            .filter(|at| *at == 0 || loop_.frames[*at] != loop_.frames[*at - 1])
            .map(|at| at as u32)
            .collect();
        let columns = (starts.len() as f64).sqrt().ceil() as u32;
        let rows = (starts.len() as u32).div_ceil(columns);
        let size = (cell.0 * columns, cell.1 * rows);
        let bytes = size.0 as u64 * size.1 as u64 * 4;
        let total: u64 = self.loop_sheets.iter().map(LoopSheet::bytes).sum();
        if bytes > MOST_BYTES || total + bytes > MOST_TOTAL {
            return None;
        }
        let started = std::time::Instant::now();
        let buffer = Buffer::new(&self.device, size.0, size.1).ok()?;
        let mut texture = buffer.texture.clone();
        for (at, start) in starts.iter().enumerate() {
            let (opacity, time) = loop_.frames[*start as usize];
            let mut drawn = item.clone();
            drawn.clip = None;
            drawn.extras = None;
            drawn.effects.opacity = opacity;
            let at = at as u32;
            let corner = (
                (at % columns * cell.0) as f32,
                (at / columns * cell.1) as f32,
            );
            let bounds = Rect::new(
                item.bounds.x - (corner.0 + fraction.0) / scale,
                item.bounds.y - (corner.1 + fraction.1) / scale,
                size.0 as f32 / scale,
                size.1 as f32 / scale,
            );
            let damage = [Rect::new(
                corner.0 / scale,
                corner.1 / scale,
                cell.0 as f32 / scale,
                cell.1 as f32 / scale,
            )];
            self.loop_time = Some(time);
            let (back, result) =
                self.paint_layer(scene, vec![drawn], bounds, texture, scale, &damage, false);
            texture = back;
            self.loop_time = None;
            result.ok()?;
        }
        crate::present_log::log(|| {
            format!(
                "loop-bake {} cells {}x{} px in {:?}",
                starts.len(),
                cell.0,
                cell.1,
                started.elapsed()
            )
        });
        Some(LoopSheet {
            node: item.id,
            baked: loop_.clone(),
            buffer: Rc::new(buffer),
            kind: item.kind.clone(),
            params,
            scale,
            size: (item.bounds.width, item.bounds.height),
            cell,
            fraction,
            starts,
            columns,
            rows,
            frame: (f32::NAN, f32::NAN),
            opacity: 1.,
            seen: Rect::default(),
        })
    }
}

/// Whether a layer over the frames shows what `item` would there: a lone
/// box with nothing cutting or covering it, by anything the walk knows of.
fn loop_shows(scene: &Scene, item: &PaintItem<'_>, scale: f32) -> bool {
    let bounds = item.bounds;
    let within = |clip: Rect| {
        clip.x <= bounds.x
            && clip.y <= bounds.y
            && clip.x + clip.width >= bounds.x + bounds.width
            && clip.y + clip.height >= bounds.y + bounds.height
    };
    !item.isolated
        && !item.border_pass
        && item.matrix().is_none()
        && item.effects.blur_radius <= 0.
        && item.clip.is_none_or(within)
        && Region::of(item).is_none_or(|region| region.covers(bounds, scale))
        && scene.children(item.id).is_empty()
        && !scene.painted_over(item.id, bounds)
}
