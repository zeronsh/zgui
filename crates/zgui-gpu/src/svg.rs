//! Bounded retained SVG pixels, shared by GPU and software renderers.
//!
//! Rasters are keyed by what they show (source, tint and pixel size), not by
//! node: every copy of an icon shares one raster (and one atlas cell), and a
//! row a virtual list mounts again finds its icons already rasterized.
use crate::GpuError;
use std::{collections::HashMap, sync::Arc};
use zgui::{
    image::ImageData,
    scene::{NodeId, NodeKind, Scene},
    svg::SvgData,
};
const MAX_ENTRIES: usize = 1024;
/// Rasters no node shows any more, kept (least recently used first out) for
/// rows that scroll back into view.
const UNUSED_BUDGET: usize = 256 * 1024;
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Key {
    source_id: u64,
    tint: Option<[u8; 4]>,
    width: u32,
    height: u32,
}
struct Entry {
    image: Arc<ImageData>,
    used: u64,
    /// Nodes showing this raster.
    users: u32,
}
struct Shown {
    key: Key,
    transformed: Arc<ImageData>,
}
/// Rasterized SVGs by source, size and tint, shared between the nodes
/// showing them; unused ones are kept within a byte budget.
#[derive(Default)]
pub struct SvgCache {
    entries: HashMap<Key, Entry>,
    nodes: HashMap<NodeId, Shown>,
    bytes: usize,
    clock: u64,
    rasterizations: u64,
}
impl SvgCache {
    /// Let go of nodes that are gone, and drop unused rasters past the budget.
    pub fn retain(&mut self, scene: &Scene) {
        let entries = &mut self.entries;
        self.nodes.retain(|node, shown| {
            let live = scene.contains(*node) && matches!(scene.kind(*node), NodeKind::Svg(_));
            if !live && let Some(entry) = entries.get_mut(&shown.key) {
                entry.users -= 1;
            }
            live
        });
        let mut unused: Vec<_> = self
            .entries
            .iter()
            .filter(|(_, e)| e.users == 0)
            .map(|(key, e)| (e.used, e.image.pixels().len(), *key))
            .collect();
        unused.sort_unstable_by_key(|&(used, ..)| std::cmp::Reverse(used));
        let mut kept = 0;
        for (_, bytes, key) in unused {
            if kept + bytes > UNUSED_BUDGET {
                self.entries.remove(&key);
                self.bytes -= bytes;
            } else {
                kept += bytes;
            }
        }
    }
    /// The image ids of the rasters it holds.
    pub fn image_ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.entries.values().map(|e| e.image.id())
    }
    /// The bytes its rasters take.
    pub fn bytes(&self) -> usize {
        self.bytes
    }
    /// How many rasterizations it has done.
    pub fn rasterizations(&self) -> u64 {
        self.rasterizations
    }
    /// `svg` rasterized for `node` at its size and scale, from the cache while
    /// unchanged.
    pub fn get(
        &mut self,
        node: NodeId,
        source: &Arc<SvgData>,
        width: f32,
        height: f32,
        scale: f32,
    ) -> Result<Arc<ImageData>, GpuError> {
        if ![width, height, scale]
            .iter()
            .all(|v| v.is_finite() && *v > 0.)
        {
            return Err(GpuError("invalid SVG size".into()));
        }
        let (width, height) = (
            (width * scale).ceil() as u32,
            (height * scale).ceil() as u32,
        );
        if (u64::from(width) * u64::from(height))
            .checked_mul(4)
            .is_none_or(|bytes| bytes > 32 * 1024 * 1024)
        {
            return Err(GpuError("SVG raster exceeds 32 MiB".into()));
        }
        self.clock = self.clock.wrapping_add(1);
        let key = Key {
            source_id: source.source_id(),
            tint: source.tint().map(|c| [c.0, c.1, c.2, c.3]),
            width,
            height,
        };
        if let Some(shown) = self.nodes.get_mut(&node)
            && shown.key == key
            && let Some(entry) = self.entries.get_mut(&key)
        {
            entry.used = self.clock;
            if shown.transformed.transform() != source.transform() {
                shown.transformed = Arc::new(entry.image.transformed(source.transform()));
            }
            return Ok(shown.transformed.clone());
        }
        if !self.entries.contains_key(&key) {
            let image = self.rasterize(source, width, height)?;
            let bytes = image.pixels().len();
            while (self.bytes + bytes > 32 * 1024 * 1024 || self.entries.len() >= MAX_ENTRIES)
                && !self.entries.is_empty()
            {
                let victim = *self.entries.iter().min_by_key(|(_, e)| e.used).unwrap().0;
                self.bytes -= self.entries.remove(&victim).unwrap().image.pixels().len();
                self.nodes.retain(|_, shown| shown.key != victim);
            }
            self.bytes += bytes;
            self.entries.insert(
                key,
                Entry {
                    image,
                    used: 0,
                    users: 0,
                },
            );
        }
        let entry = self.entries.get_mut(&key).expect("raster");
        entry.used = self.clock;
        entry.users += 1;
        let transformed = if source.transform() == entry.image.transform() {
            entry.image.clone()
        } else {
            Arc::new(entry.image.transformed(source.transform()))
        };
        let previous = self.nodes.insert(
            node,
            Shown {
                key,
                transformed: transformed.clone(),
            },
        );
        if let Some(previous) = previous
            && let Some(entry) = self.entries.get_mut(&previous.key)
        {
            entry.users -= 1;
        }
        Ok(transformed)
    }
    fn rasterize(
        &mut self,
        source: &SvgData,
        width: u32,
        height: u32,
    ) -> Result<Arc<ImageData>, GpuError> {
        let image = crate::assets::decode_svg(source.bytes(), width, height)?;
        self.rasterizations += 1;
        let Some(color) = source.tint() else {
            return Ok(image);
        };
        let mut pixels = image.pixels().to_vec();
        for p in pixels.as_chunks_mut::<4>().0 {
            p[0] = color.0;
            p[1] = color.1;
            p[2] = color.2;
            p[3] = ((u16::from(p[3]) * u16::from(color.3) + 127) / 255) as u8;
        }
        Ok(Arc::new(
            ImageData::new(width, height, pixels).map_err(|e| GpuError(e.into()))?,
        ))
    }
}

// They rasterize SVG documents.
#[cfg(all(test, feature = "svg"))]
mod tests {
    use super::*;
    #[test]
    fn unchanged_svg_reuses_transformed_handle_without_rasterization() {
        let mut scene = Scene::new(100., 100.);
        let source=Arc::new(SvgData::new(&b"<svg xmlns='http://www.w3.org/2000/svg' width='10' height='10'><rect width='10' height='10' fill='red'/></svg>"[..]).unwrap());
        let node = scene.append(
            scene.root(),
            NodeKind::Svg(source.clone()),
            Default::default(),
        );
        let mut cache = SvgCache::default();
        let first = cache.get(node, &source, 20., 20., 1.).unwrap();
        let second = cache.get(node, &source, 20., 20., 1.).unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        let rotated = Arc::new(source.transformed(zgui::affine::Affine::rotation(0.5)));
        let third = cache.get(node, &rotated, 20., 20., 1.).unwrap();
        assert!(!Arc::ptr_eq(&first, &third));
        let fourth = cache.get(node, &rotated, 20., 20., 1.).unwrap();
        assert!(Arc::ptr_eq(&third, &fourth));
        assert_eq!(cache.rasterizations(), 1);
    }
    #[test]
    fn removed_svg_source_drops_before_next_paint_and_transform_keeps_pixels() {
        let mut scene = Scene::new(100., 100.);
        let bytes:Arc<[u8]>=Arc::from(&b"<svg xmlns='http://www.w3.org/2000/svg' width='10' height='10'><rect width='10' height='10'/></svg>"[..]);
        let source = Arc::new(SvgData::new(bytes.clone()).unwrap());
        let weak = Arc::downgrade(&source);
        let transformed = Arc::new(source.transformed(zgui::affine::Affine::rotation(0.3)));
        let node = scene.append(
            scene.root(),
            NodeKind::Svg(source.clone()),
            Default::default(),
        );
        let mut cache = SvgCache::default();
        let first = cache.get(node, &source, 20., 20., 1.).unwrap();
        scene.set_kind(node, NodeKind::Svg(transformed.clone()));
        drop(source);
        assert!(weak.upgrade().is_none());
        let second = cache.get(node, &transformed, 20., 20., 1.).unwrap();
        assert_eq!(first.id(), second.id());
        assert_eq!(cache.rasterizations(), 1);
        scene.remove(node);
        drop(transformed);
        assert_eq!(
            Arc::strong_count(&bytes),
            1,
            "cache must not keep SVG byte storage alive"
        );
        assert!(
            cache.bytes() > 0,
            "test runs before next render retention pass"
        );
    }
    #[test]
    fn tiny_svg_rasters_obey_entry_count_and_lru_limit() {
        let mut scene = Scene::new(100., 100.);
        let source = Arc::new(
            SvgData::new(&b"<svg xmlns='http://www.w3.org/2000/svg' width='1' height='1'/>"[..])
                .unwrap(),
        );
        let mut cache = SvgCache::default();
        let mut first = None;
        for _ in 0..=MAX_ENTRIES {
            // A distinct source each time: equal ones share a raster.
            let source = Arc::new(SvgData::new(source.bytes().to_vec()).unwrap());
            let node = scene.append(
                scene.root(),
                NodeKind::Svg(source.clone()),
                Default::default(),
            );
            first.get_or_insert(node);
            cache.get(node, &source, 1., 1., 1.).unwrap();
        }
        assert_eq!(cache.entries.len(), MAX_ENTRIES);
        assert_eq!(cache.bytes(), MAX_ENTRIES * 4);
        assert!(!cache.nodes.contains_key(&first.unwrap()));
    }
    #[test]
    fn equal_icons_share_a_raster_that_outlives_a_remount() {
        let mut scene = Scene::new(100., 100.);
        let source = Arc::new(
            SvgData::new(&b"<svg xmlns='http://www.w3.org/2000/svg' width='10' height='10'><rect width='10' height='10'/></svg>"[..])
                .unwrap(),
        );
        let red = Arc::new(source.tinted(zgui::scene::Color(255, 0, 0, 255)));
        let mut cache = SvgCache::default();
        let mount = |scene: &mut Scene, source: &Arc<SvgData>| {
            scene.append(
                scene.root(),
                NodeKind::Svg(source.clone()),
                Default::default(),
            )
        };
        let (a, b, c) = (
            mount(&mut scene, &red),
            mount(&mut scene, &red),
            mount(&mut scene, &source),
        );
        let first = cache.get(a, &red, 10., 10., 2.).unwrap();
        assert!(Arc::ptr_eq(
            &first,
            &cache.get(b, &red, 10., 10., 2.).unwrap()
        ));
        cache.get(c, &source, 10., 10., 2.).unwrap();
        cache.get(c, &source, 10., 10., 1.).unwrap();
        assert_eq!(cache.rasterizations(), 3, "tint and size are distinct");
        // A row scrolled out and back in: its icon is not rasterized again.
        scene.remove(a);
        scene.remove(b);
        cache.retain(&scene);
        assert_eq!(cache.entries.len(), 3, "a small unused raster is kept");
        let again = mount(&mut scene, &red);
        assert_eq!(
            cache.get(again, &red, 10., 10., 2.).unwrap().id(),
            first.id()
        );
        assert_eq!(cache.rasterizations(), 3);
        // Past the budget, rasters no node shows are released.
        scene.remove(again);
        cache.retain(&scene);
        let big = mount(&mut scene, &source);
        cache.get(big, &source, 400., 400., 1.).unwrap();
        scene.remove(big);
        cache.retain(&scene);
        assert_eq!(
            cache.entries.len(),
            3,
            "the large unused raster is released"
        );
        assert!(cache.bytes() < 4096);
        assert_eq!(
            cache.bytes(),
            cache
                .entries
                .values()
                .map(|e| e.image.pixels().len())
                .sum::<usize>()
        );
    }
}
