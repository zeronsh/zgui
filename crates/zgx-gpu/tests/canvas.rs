use std::sync::Arc;
use zgx::{
    canvas::*,
    scene::{Color, Layout, NodeKind, Rect, Scene, Style},
};
use zgx_gpu::GpuRenderer;
fn drawing(width: f32, alternate: bool) -> Canvas {
    let mut canvas = Canvas::new();
    canvas.fill(
        Path::rectangle(Rect::new(0., 0., width, 80.)),
        Brush::linear(
            Point::new(0., 0.),
            Point::new(width, 0.),
            vec![
                GradientStop {
                    offset: 0.,
                    color: Color(255, 0, 0, 255),
                },
                GradientStop {
                    offset: 1.,
                    color: Color(0, 0, 255, 255),
                },
            ],
        )
        .unwrap(),
    );
    let mut path = Path::builder();
    path.move_to(10., 40.)
        .quadratic_to(25., 0., 45., 40.)
        .cubic_to(
            Point::new(55., 80.),
            Point::new(65., 0.),
            Point::new(85., 40.),
        );
    if alternate {
        path.arc(Point::new(60., 40.), 15., 0., std::f32::consts::PI);
    }
    canvas.stroke(
        path.build().unwrap(),
        Color(255, 255, 255, 255),
        Stroke {
            width: 4.,
            cap: LineCap::Round,
            dash: vec![8., 3.],
            ..Default::default()
        },
    );
    canvas.fill(
        Path::rectangle(Rect::new(25., 55., 30., 15.)),
        Brush::Slash {
            background: Color(0, 0, 0, 255),
            foreground: Color(0, 255, 0, 255),
            spacing: 8.,
            width: 3.,
        },
    );
    canvas
}
#[test]
fn paths_gradients_patterns_retain_pixels_and_restore_damage() {
    for isolated in [false, true] {
        let mut scene = Scene::new(140., 100.);
        scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
        let node = scene.append(
            scene.root(),
            NodeKind::Canvas(Arc::new(drawing(100., false))),
            Style {
                width: Some(100.),
                height: Some(80.),
                ..Default::default()
            },
        );
        scene.set_isolated(node, isolated);
        let mut gpu = GpuRenderer::new(140, 100).unwrap();
        let report = scene.flush();
        let stats = gpu.render(&scene, &report.damage).unwrap();
        assert_eq!(stats.canvas_rasterizations, 1);
        assert_eq!(stats.image_uploads, 1);
        let bytes = gpu.readback().unwrap();
        assert!(bytes[4 * (10 * 140 + 5)] > 230);
        assert!(bytes[4 * (10 * 140 + 95) + 2] > 230);
        let stats = gpu
            .render(&scene, &[Rect::new(0., 0., 140., 100.)])
            .unwrap();
        assert_eq!(stats.canvas_rasterizations, 0);
        assert_eq!(stats.image_uploads, 0);
        scene.set_kind(node, NodeKind::Canvas(Arc::new(drawing(100., true))));
        let report = scene.flush();
        assert_eq!(report.layout_nodes, 0);
        assert_eq!(
            gpu.render(&scene, &report.damage)
                .unwrap()
                .canvas_rasterizations,
            1
        );
        let mut fresh = GpuRenderer::new(140, 100).unwrap();
        fresh
            .render(&scene, &[Rect::new(0., 0., 140., 100.)])
            .unwrap();
        assert_eq!(gpu.readback().unwrap(), fresh.readback().unwrap());
        scene.remove(node);
        let report = scene.flush();
        gpu.render(&scene, &report.damage).unwrap();
        assert_eq!(gpu.debug_cache_stats().canvas_raster_bytes, 0);
    }
}
#[test]
fn bounded_raster_rejects_invalid_geometry_and_preserves_alpha() {
    let mut canvas = Canvas::new();
    canvas.fill(
        Path::rectangle(Rect::new(0., 0., 20., 20.)),
        Color(255, 80, 20, 128),
    );
    let image = zgx_gpu::canvas::rasterize(&canvas, 20., 20., 2.).unwrap();
    assert_eq!((image.width(), image.height()), (40, 40));
    let p = &image.pixels()[4 * (10 * 40 + 10)..][..4];
    assert_eq!(p[0], 255);
    assert_eq!(p[3], 128);
    assert!(zgx_gpu::canvas::rasterize(&canvas, 100000., 100000., 1.).is_err());
    assert!(zgx_gpu::canvas::rasterize(&canvas, f32::NAN, 20., 1.).is_err());
    let mut path = Path::builder();
    path.move_to(f32::NAN, 0.);
    assert!(path.build().is_err());
}

/// A canvas of simple shapes (rectangles, rounded or not, circles, their
/// strokes, lines along an axis) draws as quads, no raster made, and looks
/// as its raster does: the same within antialiasing at its edges.
#[test]
fn simple_shapes_draw_as_quads_and_look_as_their_raster_does() {
    let shapes = |raster: bool| {
        let mut canvas = Canvas::new();
        let r = |x, y, w, h| Rect::new(x, y, w, h);
        canvas.fill(
            Path::rectangle(r(0., 0., 160., 120.)),
            Color(30, 30, 40, 255),
        );
        canvas.fill(
            Path::rounded_rect(r(10.25, 8.5, 50., 30.3), 6.).unwrap(),
            Color(80, 140, 255, 255),
        );
        canvas.stroke(
            Path::rounded_rect(r(10.25, 8.5, 50., 30.3), 6.).unwrap(),
            Color(255, 255, 255, 255),
            Stroke {
                width: 1.5,
                ..Default::default()
            },
        );
        canvas.fill(
            Path::ellipse(r(80.4, 10.1, 32., 32.)).unwrap(),
            Color(255, 120, 60, 255),
        );
        canvas.stroke(
            Path::ellipse(r(80.4, 10.1, 32., 32.)).unwrap(),
            Color(20, 20, 20, 255),
            Stroke {
                width: 2.,
                ..Default::default()
            },
        );
        canvas.stroke(
            Path::rectangle(r(20.5, 60.5, 40., 30.)),
            Color(0, 220, 120, 255),
            Stroke {
                width: 3.,
                ..Default::default()
            },
        );
        canvas.stroke(
            Path::line(Point::new(70., 70.25), Point::new(150., 70.25)).unwrap(),
            Color(255, 60, 140, 255),
            Stroke {
                width: 1.,
                ..Default::default()
            },
        );
        canvas.stroke(
            Path::line(Point::new(120.6, 80.), Point::new(120.6, 115.)).unwrap(),
            Color(255, 230, 0, 255),
            Stroke {
                width: 2.,
                cap: LineCap::Round,
                ..Default::default()
            },
        );
        if raster {
            // Something no quad draws (an empty path): the whole canvas
            // rasterizes, the reference.
            canvas.fill(Path::builder().build().unwrap(), Color(0, 0, 0, 255));
        }
        canvas
    };
    for scale in [1., 2.] {
        let render = |raster: bool| {
            let mut scene = Scene::new(170., 130.);
            scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
            scene.append(
                scene.root(),
                NodeKind::Canvas(Arc::new(shapes(raster))),
                Style {
                    width: Some(160.),
                    height: Some(120.),
                    ..Default::default()
                },
            );
            let mut gpu = GpuRenderer::new((170. * scale) as u32, (130. * scale) as u32).unwrap();
            gpu.set_scale_factor(scale);
            let report = scene.flush();
            let stats = gpu.render(&scene, &report.damage).unwrap();
            (stats, gpu.readback().unwrap())
        };
        let (quads, drawn) = render(false);
        let (rasters, reference) = render(true);
        assert_eq!(quads.canvas_rasterizations, 0, "drawn as quads");
        assert!(quads.canvas_quads >= 8, "{}", quads.canvas_quads);
        assert_eq!(rasters.canvas_rasterizations, 1);
        let mut off = 0;
        let mut worst = 0;
        for (a, b) in drawn.chunks(4).zip(reference.chunks(4)) {
            let d = a.iter().zip(b).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
            worst = worst.max(d);
            if d > 6 {
                off += 1;
            }
        }
        // Insides and straight edges match; curves and line ends differ by
        // their antialiasing alone (the quad's exact circle, the raster's
        // flattened cubic arcs and coverage steps), on their edge pixels.
        let pixels = drawn.len() / 4;
        assert!(
            off * 50 < pixels,
            "scale {scale}: {off} of {pixels} pixels differ"
        );
        assert!(worst <= 64, "scale {scale}: worst {worst}");
    }
}

/// Strokes of straight lines running left to right (a sparkline's, a
/// chart's) draw as polyline quads, no raster made, and look as their raster
/// does: joins (miter, past its limit, bevel, round), caps, translucent
/// strokes over themselves, one drawn right to left. One that turns back
/// rasterizes.
#[test]
fn monotonic_polylines_draw_as_quads_and_look_as_their_raster_does() {
    let line = |points: &[(f32, f32)]| {
        let mut path = Path::builder();
        path.move_to(points[0].0, points[0].1);
        for &(x, y) in &points[1..] {
            path.line_to(x, y);
        }
        path.build().unwrap()
    };
    let strokes = |raster: bool, back: bool| {
        let mut canvas = Canvas::new();
        canvas.fill(
            Path::rectangle(Rect::new(0., 0., 160., 120.)),
            Color(30, 30, 40, 255),
        );
        // A sparkline: sixty points across eighty, mitred.
        let spark: Vec<(f32, f32)> = (0..60)
            .map(|k| {
                let t = k as f32;
                (
                    4.3 + t * 80. / 59.,
                    22. + 9. * (t * 0.7).sin() + 4. * (t * 2.3).cos(),
                )
            })
            .collect();
        canvas.stroke(
            line(&spark),
            Color(63, 185, 80, 255),
            Stroke {
                width: 1.5,
                ..Default::default()
            },
        );
        // Sharp turns: a miter past its limit bevels.
        canvas.stroke(
            line(&[
                (95., 40.),
                (110., 8.),
                (118., 40.),
                (125., 12.5),
                (150.5, 30.),
            ]),
            Color(229, 83, 75, 255),
            Stroke {
                width: 3.,
                miter_limit: 2.,
                ..Default::default()
            },
        );
        // Translucent and round over itself, then bevelled with square caps,
        // drawn right to left.
        canvas.stroke(
            line(&[(10., 90.), (30., 55.), (45., 100.), (70., 60.)]),
            Color(255, 255, 255, 140),
            Stroke {
                width: 6.,
                cap: LineCap::Round,
                join: LineJoin::Round,
                ..Default::default()
            },
        );
        canvas.stroke(
            line(&[(150., 110.), (120., 70.5), (100., 104.), (82.25, 66.)]),
            Color(80, 160, 255, 200),
            Stroke {
                width: 4.,
                cap: LineCap::Square,
                join: LineJoin::Bevel,
                ..Default::default()
            },
        );
        if back {
            canvas.stroke(
                line(&[(10., 10.), (40., 20.), (20., 30.)]),
                Color(255, 255, 255, 255),
                Stroke::default(),
            );
        }
        if raster {
            canvas.fill(Path::builder().build().unwrap(), Color(0, 0, 0, 255));
        }
        canvas
    };
    for scale in [1., 2.] {
        let render = |raster: bool, back: bool| {
            let mut scene = Scene::new(170., 130.);
            scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
            scene.append(
                scene.root(),
                NodeKind::Canvas(Arc::new(strokes(raster, back))),
                Style {
                    width: Some(160.),
                    height: Some(120.),
                    ..Default::default()
                },
            );
            let mut gpu = GpuRenderer::new((170. * scale) as u32, (130. * scale) as u32).unwrap();
            gpu.set_scale_factor(scale);
            let report = scene.flush();
            let stats = gpu.render(&scene, &report.damage).unwrap();
            (stats, gpu.readback().unwrap())
        };
        let (quads, drawn) = render(false, false);
        let (rasters, reference) = render(true, false);
        assert_eq!(quads.canvas_rasterizations, 0, "drawn as quads");
        assert_eq!(quads.canvas_quads, 5);
        assert_eq!(rasters.canvas_rasterizations, 1);
        assert_eq!(render(false, true).0.canvas_rasterizations, 1, "turns back");
        let (mut off, mut worst, mut stroked) = (0, 0, 0);
        for (a, b) in drawn.chunks(4).zip(reference.chunks(4)) {
            let d = a.iter().zip(b).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
            worst = worst.max(d);
            stroked += usize::from(b != [30, 30, 40, 255] && b != [0, 0, 0, 255]);
            if d > 6 {
                off += 1;
            }
        }
        // Lines are mostly edge: count pixels against those the strokes
        // cover, which differ by their antialiasing alone. The most where
        // the raster's area coverage sees two edges in a pixel (the narrow
        // V inside a sharp turn, a bevel's corners) and the distance the
        // nearest alone: measured 8% and 4% of them, 67 and 70 at worst.
        assert!(
            off * 10 < stroked,
            "scale {scale}: {off} of {stroked} stroked pixels differ (worst {worst})"
        );
        assert!(
            worst <= 80,
            "scale {scale}: worst {worst} ({off} of {stroked})"
        );
    }
}
