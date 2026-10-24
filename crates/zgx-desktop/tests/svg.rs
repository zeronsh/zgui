use std::sync::Arc;
use zgx::{affine::Affine, scene::*, svg::SvgData};
use zgx_desktop::raster::Raster;

#[test]
fn software_svg_tint_transform_clip_and_removal_match_full_repaint() {
    let source = Arc::new(SvgData::new(&br##"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="1"><rect width="1" height="1" fill="red"/><rect x="1" width="1" height="1" fill="green"/></svg>"##[..]).unwrap());
    for clipped in [false, true] {
        for isolated in [false, true] {
            let mut scene = Scene::new(120., 120.);
            scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
            let parent = scene.append(
                scene.root(),
                NodeKind::Container(Layout::Overlay),
                Style {
                    width: Some(40.),
                    height: Some(20.),
                    clip: clipped,
                    ..Default::default()
                },
            );
            scene.set_transform(parent, Transform { x: 40., y: 40. });
            scene.set_isolated(parent, isolated);
            let node = scene.append(
                parent,
                NodeKind::Svg(source.clone()),
                Style {
                    width: Some(40.),
                    height: Some(20.),
                    ..Default::default()
                },
            );
            let mut raster = Raster::new(120, 120);
            let report = scene.flush();
            raster.render(&scene, &report.damage);
            assert_eq!(raster.pixels[45 * 120 + 50], 0xff0000);
            for svg in [
                source.transformed(Affine::rotation(std::f32::consts::FRAC_PI_2)),
                source.tinted(Color(20, 40, 255, 255)),
                source.transformed(Affine::scale(0., 1.)),
            ] {
                scene.set_kind(node, NodeKind::Svg(Arc::new(svg)));
                let report = scene.flush();
                assert_eq!(report.layout_nodes, 0);
                raster.render(&scene, &report.damage);
                let mut fresh = Raster::new(120, 120);
                fresh.render(&scene, &[Rect::new(0., 0., 120., 120.)]);
                assert_eq!(
                    raster.pixels, fresh.pixels,
                    "clip={clipped}, isolated={isolated}"
                );
            }
            scene.remove(parent);
            let report = scene.flush();
            raster.render(&scene, &report.damage);
            let mut fresh = Raster::new(120, 120);
            fresh.render(&scene, &[Rect::new(0., 0., 120., 120.)]);
            assert_eq!(raster.pixels, fresh.pixels);
        }
    }
}
