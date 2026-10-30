//! Looping animations played by Core Animation (`Scene::set_baked_loop`).
//!
//! Any Metal submission holds about 176 MB of driver memory for the next
//! 8 to 13 seconds (M5, macOS 26.4), so a spinner drawn frame by frame holds
//! it for as long as it spins. A declared loop's frames are known ahead:
//! they are drawn once into a small sheet, and a layer over the frames plays
//! them, with no submission, no commit and no wake while it does.
//!
//! The item leaves Metal's paint only where nothing cuts it: in a clip or
//! region that changes none of its pixels. Elsewhere it draws as it did.
use crate::layer_present::{Buffer, LoopShown};
use crate::{GpuRenderer, Region};
use std::rc::Rc;
use zgx::scene::{BakedLoop, NodeId, PaintItem, Rect, Scene};

/// The most bytes a loop's sheet takes: a loop past it stays on Metal.
const MOST_BYTES: u64 = 4 << 20;

/// A loop's frames, drawn.
pub(crate) struct LoopSheet {
    node: NodeId,
    baked: Rc<BakedLoop>,
    buffer: Buffer,
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
impl LoopSheet {
    fn key(&self) -> u64 {
        self.node.number() as u64
    }
    fn shown(&self) -> LoopShown<'_> {
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
        LoopShown {
            key: self.key(),
            surface: &self.buffer.surface,
            rects,
            times,
            duration,
            phase: self.baked.started.elapsed().as_secs_f64() % duration,
        }
    }
}

impl GpuRenderer {
    /// `items` less the looping ones a layer plays, those layers placed,
    /// their sheets drawn if new.
    pub(crate) fn place_loops<'a>(
        &mut self,
        scene: &Scene,
        mut items: Vec<PaintItem<'a>>,
    ) -> Vec<PaintItem<'a>> {
        let baked = scene.baked_loops();
        if baked.is_empty() && self.loop_sheets.is_empty() {
            return items;
        }
        self.loop_sheets.retain(|sheet| {
            baked
                .iter()
                .any(|(node, loop_)| *node == sheet.node && Rc::ptr_eq(loop_, &sheet.baked))
        });
        let scale = self.scale;
        let mut placed = Vec::new();
        items.retain(|item| {
            let Some((_, loop_)) = baked.iter().find(|(node, _)| *node == item.id) else {
                return true;
            };
            let within = |clip: Rect, b: Rect| {
                clip.x <= b.x
                    && clip.y <= b.y
                    && clip.x + clip.width >= b.x + b.width
                    && clip.y + clip.height >= b.y + b.height
            };
            let uncut = item.clip.is_none_or(|clip| within(clip, item.bounds))
                && Region::of(item).is_none_or(|region| region.covers(item.bounds, scale));
            if uncut && !item.isolated && !item.border_pass && item.matrix().is_none() {
                placed.push((item.clone(), loop_.clone()));
                return false;
            }
            true
        });
        let mut changed = Vec::new();
        for (item, loop_) in placed {
            let bounds = item.bounds;
            if std::env::var_os("ZGX_BAKE_LOG").is_some() {
                eprintln!(
                    "PLACE {} {:?} op={:.3} sheets={}",
                    item.id.number(),
                    bounds,
                    item.effects.opacity,
                    self.loop_sheets.len()
                );
            }
            let (x, y) = (bounds.x * scale, bounds.y * scale);
            let fraction = (x - x.floor(), y - y.floor());
            let own = scene.effects(item.id).opacity;
            let opacity = if own > 0. {
                item.effects.opacity / own
            } else {
                1.
            };
            let key = item.id.number() as u64;
            let at = self.loop_sheets.iter().position(|s| s.node == item.id);
            let stale = at.is_some_and(|at| {
                let sheet = &self.loop_sheets[at];
                sheet.fraction != fraction && sheet.seen == bounds
            });
            if at.is_none() || stale {
                self.loop_sheets.retain(|sheet| sheet.node != item.id);
                let Some(sheet) = self.bake_loop(scene, item, loop_, fraction) else {
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
        let alive: Vec<u64> = self.loop_sheets.iter().map(LoopSheet::key).collect();
        let shown: Vec<_> = self
            .loop_sheets
            .iter()
            .filter(|sheet| changed.contains(&sheet.key()))
            .map(|sheet| {
                let (w, h) = sheet.cell;
                let rect = (
                    sheet.frame.0 as f64,
                    sheet.frame.1 as f64,
                    (w as f32 / scale) as f64,
                    (h as f32 / scale) as f64,
                );
                (sheet.shown(), rect, sheet.opacity)
            })
            .collect();
        if let Some(layer) = self
            .layer_present
            .as_mut()
            .and_then(|present| present.layer.as_mut())
        {
            layer.sync_loops(&alive, &shown);
        }
        items
    }
    /// Draws one period of `item`'s `loop_` into a new sheet; `None` for a
    /// loop past `MOST_BYTES`, or with nowhere to show.
    fn bake_loop(
        &mut self,
        scene: &Scene,
        item: PaintItem<'_>,
        loop_: Rc<BakedLoop>,
        fraction: (f32, f32),
    ) -> Option<LoopSheet> {
        let scale = self.scale;
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
        if size.0 as u64 * size.1 as u64 * 4 > MOST_BYTES {
            return None;
        }
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
        Some(LoopSheet {
            node: item.id,
            baked: loop_,
            buffer,
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
