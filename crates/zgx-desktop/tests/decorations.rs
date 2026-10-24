use std::sync::Arc;
use zgx::{
    decoration::{Corners, Decoration},
    scene::*,
};
use zgx_desktop::raster::Raster;
#[test]
fn software_asymmetric_border_and_shadow_changes_match_full_frame() {
    let mut scene = Scene::new(100., 100.);
    scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
    let quad = QuadStyle {
        fill: Color(0, 200, 80, 255),
        border_color: Color(255, 255, 255, 255),
        decoration: Some(Arc::new(Decoration {
            corners: Some(Corners {
                top_left: 20.,
                ..Default::default()
            }),
            border_widths: Some(Insets {
                left: 8.,
                right: 2.,
                top: 4.,
                bottom: 6.,
            }),
            ..Default::default()
        })),
        ..Default::default()
    };
    let node = scene.append(
        scene.root(),
        NodeKind::Quad(quad),
        Style {
            width: Some(60.),
            height: Some(60.),
            ..Default::default()
        },
    );
    let mut raster = Raster::new(100, 100);
    let report = scene.flush();
    raster.render(&scene, &report.damage);
    assert_ne!(raster.pixels[101], 0xffffff);
    assert_eq!(raster.pixels[50 * 100 + 1], 0xffffff);
    assert_eq!(raster.pixels[30 * 100 + 20], 0x00c850);
    scene.set_kind(
        node,
        NodeKind::Quad(QuadStyle {
            fill: Color(200, 20, 30, 255),
            ..Default::default()
        }),
    );
    let report = scene.flush();
    raster.render(&scene, &report.damage);
    let mut fresh = Raster::new(100, 100);
    fresh.render(&scene, &[Rect::new(0., 0., 100., 100.)]);
    assert_eq!(raster.pixels, fresh.pixels);
}
