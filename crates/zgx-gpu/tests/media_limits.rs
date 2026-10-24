use std::sync::Arc;
use zgx::{canvas::Canvas, scene::Scene, svg::SvgData};
#[test]
fn finite_extreme_raster_dimensions_return_errors_without_overflow_or_allocation() {
    let source = b"<svg xmlns='http://www.w3.org/2000/svg' width='1' height='1'/>";
    assert!(zgx_gpu::assets::decode_svg(source, u32::MAX, u32::MAX).is_err());
    let canvas = Canvas::default();
    for (w, h, scale) in [
        (f32::MAX, f32::MAX, 1.),
        (f32::MAX, 100., f32::MAX),
        (1., f32::MAX, 1.),
    ] {
        assert!(zgx_gpu::canvas::rasterize(&canvas, w, h, scale).is_err());
        let mut cache = zgx_gpu::svg::SvgCache::default();
        let source = Arc::new(SvgData::new(&source[..]).unwrap());
        assert!(
            cache
                .get(Scene::new(1., 1.).root(), &source, w, h, scale)
                .is_err()
        );
        assert_eq!(cache.bytes(), 0);
        assert_eq!(cache.rasterizations(), 0);
    }
}
