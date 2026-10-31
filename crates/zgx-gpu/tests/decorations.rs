use std::sync::Arc;
use zgx::{canvas::GradientStop, decoration::*, scene::*};
use zgx_gpu::GpuRenderer;
fn style(angle: f32) -> QuadStyle {
    QuadStyle {
        fill: Color(20, 40, 80, 255),
        border_color: Color(255, 255, 255, 255),
        decoration: Some(Arc::new(Decoration {
            background: Some(Background::Linear {
                angle,
                stops: vec![
                    GradientStop {
                        offset: 0.,
                        color: Color(255, 0, 0, 255),
                    },
                    GradientStop {
                        offset: 1.,
                        color: Color(0, 0, 255, 255),
                    },
                ]
                .into(),
            }),
            corners: Some(Corners {
                top_left: 20.,
                top_right: 3.,
                bottom_right: 14.,
                bottom_left: 0.,
            }),
            border_widths: Some(Insets {
                left: 8.,
                right: 2.,
                top: 4.,
                bottom: 6.,
            }),
            shadows: Some(
                vec![
                    BoxShadow {
                        color: Color(0, 255, 0, 180),
                        offset: Transform { x: -12., y: 0. },
                        blur_radius: 2.,
                        spread: 1.,
                    },
                    BoxShadow {
                        color: Color(255, 0, 255, 180),
                        offset: Transform { x: 12., y: 0. },
                        blur_radius: 2.,
                        spread: 1.,
                    },
                ]
                .into(),
            ),
            ..Default::default()
        })),
        ..Default::default()
    }
}
#[test]
fn detailed_panels_cache_resize_and_restore_all_shadow_extents() {
    for isolated in [false, true] {
        let mut scene = Scene::new(160., 130.);
        scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
        let node = scene.append(
            scene.root(),
            NodeKind::Quad(style(0.)),
            Style {
                width: Some(80.),
                height: Some(70.),
                ..Default::default()
            },
        );
        scene.set_transform(node, Transform { x: 40., y: 30. });
        scene.set_isolated(node, isolated);
        let mut gpu = GpuRenderer::new(160, 130).unwrap();
        let report = scene.flush();
        let stats = gpu.render(&scene, &report.damage).unwrap();
        // A gradient straight across draws as a ramp on the GPU: no raster.
        assert_eq!(stats.canvas_rasterizations, 0);
        let data = gpu.readback().unwrap();
        let pixel = |x: usize, y: usize| &data[(y * 160 + x) * 4..][..4];
        assert!(pixel(50, 65)[0] > 200);
        assert!(pixel(110, 65)[2] > 200);
        assert!(pixel(42, 70)[0] > 245 && pixel(42, 70)[1] > 245);
        assert!(pixel(30, 60)[1] > 50);
        assert!(pixel(128, 60)[0] > 50 && pixel(128, 60)[2] > 50);
        let stats = gpu
            .render(&scene, &[Rect::new(0., 0., 160., 130.)])
            .unwrap();
        assert_eq!(stats.canvas_rasterizations, 0);
        assert_eq!(stats.image_uploads, 0);
        let mut changed = style(1.);
        Arc::make_mut(changed.decoration.as_mut().unwrap()).border_style = BorderStyle::Dashed {
            length: 8.,
            gap: 5.,
        };
        scene.set_kind(node, NodeKind::Quad(changed));
        let report = scene.flush();
        assert_eq!(report.layout_nodes, 0);
        gpu.render(&scene, &report.damage).unwrap();
        let mut fresh = GpuRenderer::new(160, 130).unwrap();
        fresh
            .render(&scene, &[Rect::new(0., 0., 160., 130.)])
            .unwrap();
        assert_eq!(gpu.readback().unwrap(), fresh.readback().unwrap());
        scene.set_kind(
            node,
            NodeKind::Quad(QuadStyle {
                fill: Color(10, 20, 30, 255),
                ..Default::default()
            }),
        );
        let report = scene.flush();
        gpu.render(&scene, &report.damage).unwrap();
        let mut fresh = GpuRenderer::new(160, 130).unwrap();
        fresh
            .render(&scene, &[Rect::new(0., 0., 160., 130.)])
            .unwrap();
        assert_eq!(gpu.readback().unwrap(), fresh.readback().unwrap());
        assert_eq!(gpu.debug_cache_stats().canvas_raster_bytes, 0);
    }
}

#[test]
fn resizing_a_decorated_node_replaces_its_raster_instead_of_accumulating() {
    let mut gpu = GpuRenderer::new(320, 320).unwrap();
    let mut scene = Scene::new(320., 320.);
    let panel = scene.append(
        scene.root(),
        NodeKind::Panel {
            layout: Layout::Overlay,
            quad: style(45.),
        },
        Style {
            width: Some(200.),
            height: Some(200.),
            ..Default::default()
        },
    );
    let d = scene.flush().damage;
    gpu.render(&scene, &d).unwrap();
    let one = gpu.debug_cache_stats().image_bytes;
    // A live window resize: one new size (and raster) per frame, no structural change.
    for step in 0..400 {
        scene.set_style(
            panel,
            Style {
                width: Some(200. + (step % 100) as f32),
                height: Some(200. + (step % 100) as f32),
                ..Default::default()
            },
        );
        let d = scene.flush().damage;
        gpu.render(&scene, &d).unwrap();
    }
    let stats = gpu.debug_cache_stats();
    assert_eq!(stats.image_textures, 1, "stale rasters stay resident");
    assert!(stats.image_bytes <= one * 3);
}

#[test]
fn flat_fill_with_per_corner_radii_draws_on_the_gpu_without_a_raster() {
    let mut gpu = GpuRenderer::new(100, 100).unwrap();
    let mut scene = Scene::new(100., 100.);
    let quad = QuadStyle {
        fill: Color(255, 0, 0, 255),
        decoration: Some(Arc::new(Decoration {
            corners: Some(Corners {
                top_left: 0.,
                top_right: 30.,
                bottom_right: 30.,
                bottom_left: 0.,
            }),
            ..Default::default()
        })),
        ..Default::default()
    };
    scene.append(
        scene.root(),
        NodeKind::Panel {
            layout: Layout::Overlay,
            quad,
        },
        Style {
            width: Some(100.),
            height: Some(100.),
            ..Default::default()
        },
    );
    let d = scene.flush().damage;
    let stats = gpu.render(&scene, &d).unwrap();
    assert_eq!(stats.canvas_rasterizations, 0);
    assert_eq!(gpu.debug_cache_stats().image_textures, 0);
    let pixels = gpu.readback().unwrap();
    let at = |x: usize, y: usize| &pixels[(y * 100 + x) * 4..(y * 100 + x) * 4 + 4];
    assert_eq!(at(1, 1)[0], 255, "square top-left corner is filled");
    assert_eq!(at(1, 98)[0], 255, "square bottom-left corner is filled");
    assert_eq!(at(98, 1)[3], 0, "rounded top-right corner is cut");
    assert_eq!(at(98, 98)[3], 0, "rounded bottom-right corner is cut");
    assert_eq!(at(50, 50), &[255, 0, 0, 255]);
}

#[test]
fn gradients_along_an_axis_draw_as_ramps_matching_the_raster() {
    // The same gradient straight down (a ramp) and at a hair off it (a
    // raster): the pixels agree, and only the second rasterizes.
    let render = |angle: f32| {
        let mut scene = Scene::new(100., 200.);
        let quad = QuadStyle {
            decoration: Some(Arc::new(Decoration {
                background: Some(Background::Linear {
                    angle,
                    stops: vec![
                        GradientStop {
                            offset: 0.2,
                            color: Color(255, 0, 0, 255),
                        },
                        GradientStop {
                            offset: 0.5,
                            color: Color(0, 0, 255, 128),
                        },
                        GradientStop {
                            offset: 1.,
                            color: Color(0, 255, 0, 255),
                        },
                    ]
                    .into(),
                }),
                ..Default::default()
            })),
            ..Default::default()
        };
        scene.append(
            scene.root(),
            NodeKind::Quad(quad),
            Style {
                width: Some(100.),
                height: Some(200.),
                ..Default::default()
            },
        );
        let mut gpu = GpuRenderer::new(100, 200).unwrap();
        let report = scene.flush();
        let stats = gpu.render(&scene, &report.damage).unwrap();
        (stats.canvas_rasterizations, gpu.readback().unwrap())
    };
    let (ramps, ramp) = render(std::f32::consts::FRAC_PI_2);
    let (rasters, raster) = render(std::f32::consts::FRAC_PI_2 + 1e-4);
    assert_eq!((ramps, rasters), (0, 1));
    for y in [2, 30, 40, 60, 99, 100, 150, 197] {
        let at = |data: &[u8]| data[(y * 100 + 50) * 4..][..4].to_vec();
        let (a, b) = (at(&ramp), at(&raster));
        assert!(
            a.iter().zip(&b).all(|(a, b)| a.abs_diff(*b) <= 6),
            "row {y}: ramp {a:?}, raster {b:?}"
        );
    }
}

#[test]
fn underlined_plain_text_draws_a_line_under_each_line_of_glyphs() {
    let render = |underline: Option<zgx::text_layout::TextUnderline>| {
        let mut gpu = GpuRenderer::new(120, 40).unwrap();
        let mut scene = Scene::new(120., 40.);
        let text = scene.append(
            scene.root(),
            NodeKind::Text {
                text: "HHHH".into(),
                font_size: 20.,
                color: Color(255, 255, 255, 255),
            },
            Style {
                width: Some(120.),
                height: Some(40.),
                ..Default::default()
            },
        );
        scene.set_font(
            text,
            zgx::text_layout::FontStyle {
                underline,
                ..Default::default()
            },
        );
        let d = scene.flush().damage;
        gpu.render(&scene, &d).unwrap();
        gpu.readback().unwrap()
    };
    // The rows lit all along the text: only an underline's.
    let full_rows = |pixels: &[u8]| {
        (0..40)
            .filter(|y| (4..30).all(|x| pixels[(y * 120 + x) * 4 + 3] > 0))
            .collect::<Vec<_>>()
    };
    assert!(full_rows(&render(None)).is_empty());
    let rows = full_rows(&render(Some(zgx::text_layout::TextUnderline::new(Some(
        3.,
    )))));
    assert!(!rows.is_empty(), "a line under the glyphs");
}

/// Runs of bright and dark along `row` from `x0` to `x1` of a `width`-wide
/// readback: how many times it changes between them.
fn changes(data: &[u8], width: usize, row: usize, x0: usize, x1: usize) -> (usize, Vec<u8>) {
    let line: Vec<u8> = (x0..x1).map(|x| data[(row * width + x) * 4]).collect();
    let bright: Vec<bool> = line.iter().map(|v| *v > 128).collect();
    (bright.windows(2).filter(|w| w[0] != w[1]).count(), line)
}

/// A dotted border (`BorderStyle::DOTTED`) is round dots as wide as the
/// border, as far apart, along its middle: its top edge alternates dot and
/// gap, where a solid one is one run.
#[test]
fn dotted_borders_draw_dots_along_the_border() {
    let draw = |border_style: BorderStyle| {
        let mut scene = Scene::new(120., 60.);
        scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
        let node = scene.append(
            scene.root(),
            NodeKind::Quad(QuadStyle {
                fill: Color(0, 0, 0, 255),
                border_color: Color(255, 255, 255, 255),
                border_width: 4.,
                decoration: Some(Arc::new(Decoration {
                    border_style,
                    ..Default::default()
                })),
                ..Default::default()
            }),
            Style {
                width: Some(100.),
                height: Some(40.),
                ..Default::default()
            },
        );
        scene.set_transform(node, Transform { x: 10., y: 10. });
        let mut gpu = GpuRenderer::new(120, 60).unwrap();
        let report = scene.flush();
        gpu.render(&scene, &report.damage).unwrap();
        gpu.readback().unwrap()
    };
    // The border's middle row: y 12.
    let (solid, _) = changes(&draw(BorderStyle::Solid), 120, 12, 20, 100);
    assert_eq!(solid, 0, "solid: one run");
    let (dotted, line) = changes(&draw(BorderStyle::DOTTED), 120, 12, 20, 100);
    // Dots 4 across every 8 or so: about ten over 80 points.
    assert!((14..=24).contains(&dotted), "{dotted} changes: {line:?}");
    assert!(
        line.iter().any(|v| *v > 230) && line.iter().any(|v| *v < 25),
        "{line:?}"
    );
}

/// An outline drawn dotted or dashed (`Scene::set_outline_style`) rings the
/// box outside it as a solid one does, broken as asked, and rasterizes once;
/// a solid one stays the shader's ring (no raster), and the box under them
/// is as it was.
#[test]
fn dotted_and_dashed_outlines_ring_the_box() {
    let draw = |outline_style: BorderStyle| {
        let mut scene = Scene::new(140., 80.);
        scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
        let node = scene.append(
            scene.root(),
            NodeKind::Quad(QuadStyle {
                fill: Color(40, 40, 40, 255),
                outline: Some(Outline {
                    width: 3.,
                    color: Color(255, 255, 255, 255),
                    offset: 4.,
                }),
                ..Default::default()
            }),
            Style {
                width: Some(80.),
                height: Some(40.),
                ..Default::default()
            },
        );
        scene.set_transform(node, Transform { x: 30., y: 20. });
        scene.set_outline_style(node, outline_style);
        let mut gpu = GpuRenderer::new(140, 80).unwrap();
        let report = scene.flush();
        let first = gpu.render(&scene, &report.damage).unwrap();
        let again = gpu.render(&scene, &[Rect::new(0., 0., 140., 80.)]).unwrap();
        (
            gpu.readback().unwrap(),
            first.canvas_rasterizations,
            again.image_uploads,
        )
    };
    // The outline's middle row: 4 + 1.5 above the box's top (y 20), y 14.
    let (pixels, rasters, _) = draw(BorderStyle::Solid);
    assert_eq!(rasters, 0, "solid: the shader's ring");
    assert_eq!(changes(&pixels, 140, 14, 34, 106).0, 0, "solid: one run");
    assert_eq!(pixels[(40 * 140 + 70) * 4], 40, "the box under it");
    let (pixels, rasters, uploads) = draw(BorderStyle::DOTTED);
    assert_eq!(rasters, 1, "dotted: rasterized");
    assert_eq!(uploads, 0, "and kept");
    let (dotted, line) = changes(&pixels, 140, 14, 34, 106);
    // Dots 3 across every 6 or so: about twelve over 72 points.
    assert!((16..=30).contains(&dotted), "{dotted} changes: {line:?}");
    assert_eq!(pixels[(40 * 140 + 70) * 4], 40, "the box under it");
    let (pixels, _, _) = draw(BorderStyle::Dashed {
        length: 10.,
        gap: 6.,
    });
    let (dashed, line) = changes(&pixels, 140, 14, 34, 106);
    // Dashes 10 long every 16: four or five over 72 points.
    assert!((6..=12).contains(&dashed), "{dashed} changes: {line:?}");
    assert!(pixels[(14 * 140 + 10) * 4] < 25, "nothing past the outline");
}
