//! `ZGX_GPU_STATS=1`: once a second, print summed per-frame renderer work and
//! the retained GPU cache sizes to stderr.
use std::time::{Duration, Instant};
use zgx_gpu::{GpuRenderer, GpuStats};

pub(crate) struct GpuStatsLog {
    since: Instant,
    frames: u32,
    sum: GpuStats,
}
impl GpuStatsLog {
    pub(crate) fn from_env() -> Option<Self> {
        std::env::var_os("ZGX_GPU_STATS").map(|_| Self {
            since: Instant::now(),
            frames: 0,
            sum: GpuStats::default(),
        })
    }
    pub(crate) fn record(&mut self, frame: GpuStats, renderer: &GpuRenderer) {
        let sum = &mut self.sum;
        self.frames += 1;
        sum.draw_calls += frame.draw_calls;
        sum.render_passes += frame.render_passes;
        sum.blur_passes += frame.blur_passes;
        sum.instances += frame.instances;
        sum.glyph_uploads += frame.glyph_uploads;
        sum.geometry_rebuilds += frame.geometry_rebuilds;
        sum.vertex_buffer_allocations += frame.vertex_buffer_allocations;
        sum.damaged_pixels += frame.damaged_pixels;
        sum.layer_repaints += frame.layer_repaints;
        sum.layer_cache_hits += frame.layer_cache_hits;
        sum.layer_texture_allocations += frame.layer_texture_allocations;
        if self.since.elapsed() < Duration::from_secs(1) {
            return;
        }
        let caches = renderer.debug_cache_stats();
        // Prepared text: shaped lines, then compact measurements.
        let prepared = renderer.text_cache().borrow().bytes();
        let mb = |bytes: usize| bytes as f64 / (1024. * 1024.);
        eprintln!(
            "GPU_STATS frames={} draws={} passes={} blur_passes={} instances={} \
             damaged_mpx={:.1} geometry={} glyphs={} vbuf_allocs={} layer_repaints={} \
             layer_hits={} layer_allocs={} | layers={} ({:.1} MB) atlas={:.1} MB \
             images={:.1} MB vertices={:.1} MB shaped={:.1} MB prepared={:.1}+{:.1} MB \
             device={}",
            self.frames,
            sum.draw_calls,
            sum.render_passes,
            sum.blur_passes,
            sum.instances,
            sum.damaged_pixels as f64 / 1e6,
            sum.geometry_rebuilds,
            sum.glyph_uploads,
            sum.vertex_buffer_allocations,
            sum.layer_repaints,
            sum.layer_cache_hits,
            sum.layer_texture_allocations,
            caches.layer_textures,
            mb(caches.layer_bytes),
            mb(caches.atlas_bytes),
            mb(caches.image_bytes),
            mb(caches.vertex_buffer_bytes),
            mb(caches.shaped_bytes),
            mb(prepared.0),
            mb(prepared.1),
            renderer
                .debug_device_allocated_bytes()
                .map_or("n/a".into(), |bytes| format!(
                    "{:.1} MB",
                    mb(bytes as usize)
                )),
        );
        *self = Self {
            since: Instant::now(),
            frames: 0,
            sum: GpuStats::default(),
        };
    }
}
