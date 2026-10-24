use std::sync::Arc;
use zgx::{
    canvas::*,
    scene::{Color, Layout, NodeKind, Rect, Scene, Style},
};
use zgx_desktop::raster::Raster;
#[test]
fn software_paths_restore_old_pixels_and_clip_in_layers() {
    for isolated in [false, true] {
        let mut scene = Scene::new(100., 100.);
        scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
        let make = |x| {
            let mut c = Canvas::new();
            c.fill(
                Path::rectangle(Rect::new(x, 10., 30., 40.)),
                Color(250, 20, 10, 255),
            );
            Arc::new(c)
        };
        let node = scene.append(
            scene.root(),
            NodeKind::Canvas(make(10.)),
            Style {
                width: Some(80.),
                height: Some(80.),
                ..Default::default()
            },
        );
        scene.set_isolated(node, isolated);
        let mut raster = Raster::new(100, 100);
        let report = scene.flush();
        raster.render(&scene, &report.damage);
        assert_eq!(raster.pixels[20 * 100 + 20], 0xfa140a);
        scene.set_kind(node, NodeKind::Canvas(make(60.)));
        let report = scene.flush();
        raster.render(&scene, &report.damage);
        assert_eq!(raster.pixels[20 * 100 + 70], 0xfa140a);
        assert_ne!(raster.pixels[20 * 100 + 85], 0xfa140a);
        let mut fresh = Raster::new(100, 100);
        fresh.render(&scene, &[Rect::new(0., 0., 100., 100.)]);
        assert_eq!(raster.pixels, fresh.pixels);
    }
}
