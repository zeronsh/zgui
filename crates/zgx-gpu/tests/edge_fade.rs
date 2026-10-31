//! `fade_edges` on all four sides: a clip fades its descendants out over
//! independent bands at each edge (a horizontally scrolling row fading its
//! right 28 px), on the wgpu path and with Metal directly. A card crossing
//! a band fades as one: what it holds over its paper, not the paper
//! through it.
use zgx::scene::*;
use zgx_gpu::GpuRenderer;

fn gpu_fixture() -> std::sync::MutexGuard<'static, ()> {
    static DEVICE_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());
    DEVICE_TEST
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
const SIZE: u32 = 200;
fn alpha(pixels: &[u8], x: u32, y: u32) -> u8 {
    pixels[((y * SIZE + x) * 4 + 3) as usize]
}
/// A 160 × 80 clip at (20, 20) fading `vertical` (top, bottom) and
/// `horizontal` (left, right) bands, filled by an opaque rect.
fn faded(vertical: [f32; 2], horizontal: [f32; 2]) -> Scene {
    let mut scene = Scene::new(SIZE as f32, SIZE as f32);
    scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
    let clip = scene.append(
        scene.root(),
        NodeKind::Container(Layout::Overlay),
        Style {
            width: Some(160.),
            height: Some(80.),
            clip: true,
            fade_edges: vertical,
            fade_edges_x: horizontal,
            ..Default::default()
        },
    );
    scene.set_transform(clip, Transform { x: 20., y: 20. });
    scene.append(
        clip,
        NodeKind::Rect(Color(255, 0, 0, 255)),
        Style {
            width: Some(400.),
            height: Some(80.),
            ..Default::default()
        },
    );
    scene
}
/// How far `distance` into a band `width` wide is faded in: smoothstep,
/// level at both ends.
fn eased(distance: f32, width: f32) -> f32 {
    let t = (distance / width).clamp(0., 1.);
    t * t * (3. - 2. * t)
}
/// The alpha `distance` into a band `width` wide.
fn ramp(distance: f32, width: f32) -> u8 {
    (eased(distance, width) * 255.).round() as u8
}
fn render(direct: bool, scene: &mut Scene) -> Vec<u8> {
    let mut gpu = GpuRenderer::new(SIZE, SIZE).unwrap();
    gpu.set_direct_metal(direct);
    let damage = scene.flush().damage;
    gpu.render(scene, &damage).unwrap();
    gpu.readback().unwrap()
}

#[test]
fn a_row_fades_its_right_edge_alone() {
    let _gpu_fixture = gpu_fixture();
    for direct in [true, false] {
        let p = render(direct, &mut faded([0., 0.], [0., 28.]));
        // Opaque to 28 px from the right edge (x 180), then an eased
        // ramp down to nothing; top, bottom and left stay sharp.
        assert_eq!(alpha(&p, 150, 60), 255, "direct {direct}");
        assert_eq!(alpha(&p, 21, 60), 255);
        assert_eq!(alpha(&p, 100, 21), 255);
        assert_eq!(alpha(&p, 100, 98), 255);
        // Into the band, eased by how far in: at x 166 (its centre 13.5 px
        // from the edge), smoothstep(13.5 / 28).
        let half = alpha(&p, 166, 60);
        assert!(half.abs_diff(ramp(13.5, 28.)) <= 1, "half {half}");
        assert!(alpha(&p, 179, 60) < 5);
        let ramp: Vec<u8> = (150..180).map(|x| alpha(&p, x, 60)).collect();
        assert!(ramp.windows(2).all(|w| w[0] >= w[1]), "{ramp:?}");
    }
}

#[test]
fn all_four_edges_fade_independently() {
    let _gpu_fixture = gpu_fixture();
    for direct in [true, false] {
        let p = render(direct, &mut faded([10., 20.], [30., 40.]));
        // Each band by its own width, at pixel centres; the middle opaque.
        let near = |a: u8, b: u8| a.abs_diff(b) <= 1;
        assert!(
            near(alpha(&p, 100, 25), ramp(5.5, 10.)),
            "top {}",
            alpha(&p, 100, 25)
        );
        assert!(
            near(alpha(&p, 100, 90), ramp(9.5, 20.)),
            "bottom {}",
            alpha(&p, 100, 90)
        );
        assert!(
            near(alpha(&p, 35, 60), ramp(15.5, 30.)),
            "left {}",
            alpha(&p, 35, 60)
        );
        assert!(
            near(alpha(&p, 160, 60), ramp(19.5, 40.)),
            "right {}",
            alpha(&p, 160, 60)
        );
        assert_eq!(alpha(&p, 100, 60), 255, "direct {direct}");
    }
}

/// A 160 × 80 card at `y` in the faded clip: light paper, rounded and
/// clipping, with dark art over it.
fn papered(y: f32) -> (Scene, NodeId) {
    let mut scene = Scene::new(SIZE as f32, SIZE as f32);
    scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
    let clip = scene.append(
        scene.root(),
        NodeKind::Container(Layout::Overlay),
        Style {
            width: Some(160.),
            height: Some(160.),
            clip: true,
            fade_edges: [40., 0.],
            ..Default::default()
        },
    );
    scene.set_transform(clip, Transform { x: 20., y: 20. });
    let card = scene.append(
        clip,
        NodeKind::Panel {
            layout: Layout::Overlay,
            quad: QuadStyle {
                fill: Color(240, 238, 232, 255),
                radius: 12.,
                ..Default::default()
            },
        },
        Style {
            width: Some(160.),
            height: Some(80.),
            clip: true,
            ..Default::default()
        },
    );
    scene.set_transform(card, Transform { x: 0., y });
    let art = zgx::image::ImageData::new(4, 4, [40u8, 40, 80, 255].repeat(16)).unwrap();
    scene.append(
        card,
        NodeKind::Image(std::sync::Arc::new(art)),
        Style {
            width: Some(160.),
            height: Some(80.),
            ..Default::default()
        },
    );
    (scene, card)
}

#[test]
fn a_card_fades_as_one_across_a_band() {
    let _gpu_fixture = gpu_fixture();
    for direct in [true, false] {
        let (mut scene, _) = papered(0.);
        let p = render(direct, &mut scene);
        // Its art fades toward transparent, its own colour all the way: the
        // paper under it does not show through as a light veil.
        for y in [25, 35, 45, 55] {
            let at = ((y * SIZE + 100) * 4) as usize;
            let [r, g, b, a] = p[at..at + 4].try_into().unwrap();
            let expected = ramp(y as f32 - 20. + 0.5, 40.);
            assert!(
                a.abs_diff(expected) <= 2,
                "direct {direct}, y {y}: alpha {a}"
            );
            // Premultiplied: the art's colour scaled by its alpha.
            let of = |art: f32| (art * a as f32 / 255.).round() as u8;
            assert!(
                r.abs_diff(of(40.)) <= 2 && g.abs_diff(of(40.)) <= 2 && b.abs_diff(of(80.)) <= 2,
                "direct {direct}, y {y}: {:?}",
                [r, g, b, a]
            );
        }

        // Out of the band it paints as it is drawn, the same.
        let (mut scene, _) = papered(60.);
        let p = render(direct, &mut scene);
        let at = ((120 * SIZE + 100) * 4) as usize;
        assert_eq!(p[at..at + 4], [40, 40, 80, 255], "direct {direct}");
    }
}

#[test]
fn a_card_scrolled_through_the_band_draws_as_a_frame_drawn_afresh() {
    let _gpu_fixture = gpu_fixture();
    for direct in [true, false] {
        let (mut scene, card) = papered(0.);
        let mut gpu = GpuRenderer::new(SIZE, SIZE).unwrap();
        gpu.set_direct_metal(direct);
        let damage = scene.flush().damage;
        gpu.render(&scene, &damage).unwrap();
        for y in [60., -10., 30.5, 0.] {
            scene.set_transform(card, Transform { x: 0., y });
            let damage = scene.flush().damage;
            gpu.render(&scene, &damage).unwrap();
            let (mut fresh, _) = papered(y);
            assert_eq!(
                gpu.readback().unwrap(),
                render(direct, &mut fresh),
                "direct {direct}, y {y}"
            );
        }
    }
}

/// A clip at (20, 20), 160 × 160, its top 40 px fading (unless `fade` is
/// false), holding what `fill` adds.
fn holding(fade: bool, fill: impl FnOnce(&mut Scene, NodeId)) -> Scene {
    let mut scene = Scene::new(SIZE as f32, SIZE as f32);
    scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
    let clip = scene.append(
        scene.root(),
        NodeKind::Container(Layout::Column),
        Style {
            width: Some(160.),
            height: Some(160.),
            clip: true,
            fade_edges: if fade { [40., 0.] } else { [0., 0.] },
            ..Default::default()
        },
    );
    scene.set_transform(clip, Transform { x: 20., y: 20. });
    fill(&mut scene, clip);
    scene
}
/// That every pixel is what CSS's `mask-image` makes: what the clip holds
/// drawn unfaded, scaled once by the ramp at the pixel's centre.
fn masked_once(fill: impl Fn(&mut Scene, NodeId)) {
    let _gpu_fixture = gpu_fixture();
    for direct in [true, false] {
        let p = render(direct, &mut holding(true, &fill));
        let flat = render(direct, &mut holding(false, &fill));
        for y in 0..SIZE {
            let m = match y {
                20..60 => ramp(y as f32 - 20. + 0.5, 40.),
                _ => 255,
            };
            for x in 0..SIZE {
                let at = ((y * SIZE + x) * 4) as usize;
                for c in 0..4 {
                    let want = (flat[at + c] as f32 * m as f32 / 255.).round() as u8;
                    assert!(
                        p[at + c].abs_diff(want) <= 2,
                        "direct {direct}, ({x}, {y}) channel {c}: {} for {want}",
                        p[at + c]
                    );
                }
            }
        }
    }
}
fn light() -> Color {
    Color(240, 238, 232, 255)
}
fn dark() -> Color {
    Color(40, 40, 80, 255)
}

#[test]
fn an_overlay_over_a_filled_sibling_fades_as_one() {
    masked_once(|scene, clip| {
        let sized = |w: f32, h: f32| Style {
            width: Some(w),
            height: Some(h),
            ..Default::default()
        };
        scene.append(clip, NodeKind::Rect(light()), sized(160., 80.));
        let over = scene.append(
            clip,
            NodeKind::Rect(dark()),
            Style {
                absolute: true,
                ..sized(120., 50.)
            },
        );
        scene.set_transform(over, Transform { x: 20., y: 5. });
    });
}

#[test]
fn a_filled_column_holding_the_content_fades_as_one() {
    masked_once(|scene, clip| {
        let column = scene.append(
            clip,
            NodeKind::Panel {
                layout: Layout::Column,
                quad: QuadStyle {
                    fill: light(),
                    ..Default::default()
                },
            },
            Style {
                width: Some(160.),
                height: Some(400.),
                gap: 8.,
                ..Default::default()
            },
        );
        for _ in 0..6 {
            let card = scene.append(
                column,
                NodeKind::Panel {
                    layout: Layout::Overlay,
                    quad: QuadStyle {
                        fill: Color(255, 255, 255, 255),
                        radius: 10.,
                        ..Default::default()
                    },
                },
                Style {
                    width: Some(140.),
                    height: Some(36.),
                    clip: true,
                    ..Default::default()
                },
            );
            scene.append(
                card,
                NodeKind::Rect(dark()),
                Style {
                    width: Some(80.),
                    height: Some(36.),
                    ..Default::default()
                },
            );
        }
    });
}

#[test]
fn a_tilted_card_in_a_band_fades_as_one() {
    masked_once(|scene, clip| {
        let card = scene.append(
            clip,
            NodeKind::Panel {
                layout: Layout::Overlay,
                quad: QuadStyle {
                    fill: light(),
                    radius: 8.,
                    ..Default::default()
                },
            },
            Style {
                width: Some(120.),
                height: Some(60.),
                ..Default::default()
            },
        );
        scene.set_transform(card, Transform { x: 20., y: 4. });
        scene.set_local_transform(
            card,
            LocalTransform {
                rotate: 0.2,
                ..LocalTransform::IDENTITY
            },
        );
        scene.append(
            card,
            NodeKind::Rect(dark()),
            Style {
                width: Some(60.),
                height: Some(40.),
                ..Default::default()
            },
        );
    });
}

/// `scene` drawn with fade bands as one and item by item, at `scale`.
fn both_ways(direct: bool, scale: f32, scene: impl Fn() -> Scene) -> (Vec<u8>, Vec<u8>) {
    let draw = |bands: bool| {
        let mut scene = scene();
        let mut gpu = GpuRenderer::new(SIZE, SIZE).unwrap();
        gpu.set_direct_metal(direct);
        gpu.set_scale_factor(scale);
        gpu.set_fade_bands(bands);
        let damage = scene.flush().damage;
        gpu.render(&scene, &damage).unwrap();
        gpu.readback().unwrap()
    };
    (draw(true), draw(false))
}

#[test]
fn one_item_fades_as_it_did_item_by_item_across_the_seams() {
    // Where fading item by item was right (one opaque item), the same
    // pixels exactly: across each band's edge with the interior, at a
    // fractional scale and offset.
    let _gpu_fixture = gpu_fixture();
    for direct in [true, false] {
        for scale in [1., 1.5] {
            let (banded, by_item) = both_ways(direct, scale, || {
                let size = SIZE as f32 / scale;
                let mut scene = Scene::new(size, size);
                scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
                let clip = scene.append(
                    scene.root(),
                    NodeKind::Container(Layout::Overlay),
                    Style {
                        width: Some(100.3),
                        height: Some(90.7),
                        clip: true,
                        fade_edges: [17.3, 21.6],
                        fade_edges_x: [9.4, 13.1],
                        ..Default::default()
                    },
                );
                scene.set_transform(clip, Transform { x: 10.7, y: 12.35 });
                let item = scene.append(
                    clip,
                    NodeKind::Rect(Color(200, 30, 60, 255)),
                    Style {
                        width: Some(100.3),
                        height: Some(200.),
                        ..Default::default()
                    },
                );
                scene.set_transform(item, Transform { x: 0., y: -33.45 });
                scene
            });
            assert!(banded == by_item, "direct {direct}, scale {scale}");
        }
    }
}

#[test]
fn a_fade_within_a_band_composes_as_it_did() {
    let _gpu_fixture = gpu_fixture();
    for direct in [true, false] {
        let (banded, by_item) = both_ways(direct, 1., || {
            holding(true, |scene, clip| {
                let inner = scene.append(
                    clip,
                    NodeKind::Container(Layout::Overlay),
                    Style {
                        width: Some(120.),
                        height: Some(50.),
                        clip: true,
                        fade_edges: [0., 20.],
                        ..Default::default()
                    },
                );
                scene.append(
                    inner,
                    NodeKind::Rect(dark()),
                    Style {
                        width: Some(120.),
                        height: Some(50.),
                        ..Default::default()
                    },
                );
                scene.append(
                    clip,
                    NodeKind::Rect(light()),
                    Style {
                        width: Some(160.),
                        height: Some(60.),
                        ..Default::default()
                    },
                );
            })
        });
        assert!(banded == by_item, "direct {direct}");
    }
}

#[test]
fn rows_apart_in_a_band_fade_as_they_did_item_by_item() {
    // Nothing in a band stacks: faded item by item, which is exact there,
    // and drawn as it was.
    let _gpu_fixture = gpu_fixture();
    for direct in [true, false] {
        let (banded, by_item) = both_ways(direct, 1., || {
            holding(true, |scene, clip| {
                for i in 0..6 {
                    let row = scene.append(
                        clip,
                        NodeKind::Container(Layout::Row),
                        Style {
                            width: Some(160.),
                            height: Some(24.),
                            gap: 8.,
                            ..Default::default()
                        },
                    );
                    scene.append(
                        row,
                        NodeKind::Rect(dark()),
                        Style {
                            width: Some(16.),
                            height: Some(16.),
                            ..Default::default()
                        },
                    );
                    scene.append(
                        row,
                        NodeKind::Rect(Color(200, 60, 40, 255 - i * 30)),
                        Style {
                            width: Some(100.),
                            height: Some(10.),
                            ..Default::default()
                        },
                    );
                }
            })
        });
        assert!(banded == by_item, "direct {direct}");
    }
}

/// A faded list in a row folded shut (height 0, clipping) in a popup away
/// from the window's corner: its clip, and so its fade's box, is empty,
/// which the walk leaves at the window's origin, wholly outside the
/// popup's target once its items are moved into the popup's space. Drawn
/// without bands there, not a panic on every frame.
#[test]
fn a_fade_folded_shut_in_a_popup_draws() {
    let _gpu_fixture = gpu_fixture();
    let mut scene = Scene::new(400., 400.);
    scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
    let popup = scene.append(
        scene.root(),
        NodeKind::Container(Layout::Column),
        Style {
            width: Some(80.),
            height: Some(80.),
            ..Default::default()
        },
    );
    scene.set_transform(popup, Transform { x: 40., y: 300. });
    scene.set_popup(popup, true);
    let row = scene.append(
        popup,
        NodeKind::Container(Layout::Overlay),
        Style {
            width: Some(80.),
            height: Some(0.),
            clip: true,
            ..Default::default()
        },
    );
    let list = scene.append(
        row,
        NodeKind::Container(Layout::Overlay),
        Style {
            width: Some(80.),
            height: Some(60.),
            clip: true,
            fade_edges: [16., 16.],
            ..Default::default()
        },
    );
    // Stacked in a band, so the run would draw its bands as one.
    for color in [light(), dark()] {
        scene.append(
            list,
            NodeKind::Rect(color),
            Style {
                width: Some(80.),
                height: Some(60.),
                ..Default::default()
            },
        );
    }
    let damage = scene.flush().damage;
    let mut gpu = GpuRenderer::new(80, 80).unwrap();
    gpu.set_popup(popup, Rect::new(40., 300., 80., 80.));
    gpu.render(&scene, &damage).unwrap();
    gpu.render(&scene, &[Rect::new(0., 0., 400., 400.)])
        .unwrap();
}

/// A 160 × 160 clip at (20, 20) fading 40 px at its top and bottom, holding
/// a clip of its own 160 × 60 at its top, fading `inner_y` (top, bottom)
/// and `inner_x` (left, right), filled by `fill` (one rect, or two stacked
/// to draw the bands as one).
fn nested(inner_y: [f32; 2], inner_x: [f32; 2], stacked: bool) -> Scene {
    let mut scene = Scene::new(SIZE as f32, SIZE as f32);
    scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
    let outer = scene.append(
        scene.root(),
        NodeKind::Container(Layout::Overlay),
        Style {
            width: Some(160.),
            height: Some(160.),
            clip: true,
            fade_edges: [40., 40.],
            ..Default::default()
        },
    );
    scene.set_transform(outer, Transform { x: 20., y: 20. });
    let inner = scene.append(
        outer,
        NodeKind::Container(Layout::Overlay),
        Style {
            width: Some(160.),
            height: Some(60.),
            clip: true,
            fade_edges: inner_y,
            fade_edges_x: inner_x,
            ..Default::default()
        },
    );
    let colors: &[Color] = match stacked {
        true => &[Color(40, 40, 80, 255), Color(255, 0, 0, 255)],
        false => &[Color(255, 0, 0, 255)],
    };
    for color in colors {
        scene.append(
            inner,
            NodeKind::Rect(*color),
            Style {
                width: Some(160.),
                height: Some(60.),
                ..Default::default()
            },
        );
    }
    scene
}

#[test]
fn a_fade_inside_a_fade_takes_both() {
    // A label fading its right end in a list fading its top (x inside y),
    // and a fade inside another on the same edge (y inside y): each pixel
    // takes both masks, multiplied, on both paths, one item or stacked
    // (bands drawn as one). The nearest fade alone cut the outer's off.
    let _gpu_fixture = gpu_fixture();
    let near = |got: u8, want: f32| (got as f32 - want * 255.).abs() <= 2.;

    for direct in [true, false] {
        for stacked in [false, true] {
            // x inside y: the outer's top band (y 20 to 60), the inner's
            // right band (x 150 to 180).
            let p = render(direct, &mut nested([0., 0.], [0., 30.], stacked));
            let outer = eased(10.5, 40.);
            let a = alpha(&p, 60, 30);
            assert!(
                near(a, outer),
                "outer alone {a} (direct {direct}, stacked {stacked})"
            );
            let a = alpha(&p, 165, 30);
            let both = outer * eased(14.5, 30.);
            assert!(
                near(a, both),
                "both {a} want {} (direct {direct})",
                both * 255.
            );
            // Below the outer's band, the inner's right band alone.
            let a = alpha(&p, 165, 70);
            assert!(near(a, eased(14.5, 30.)), "inner alone {a}");
            // y inside y: both fade the top, over 40 and 20 px.
            let p = render(direct, &mut nested([20., 0.], [0., 0.], stacked));
            let a = alpha(&p, 60, 30);
            let both = eased(10.5, 40.) * eased(10.5, 20.);
            assert!(near(a, both), "y in y {a} want {}", both * 255.);
            let a = alpha(&p, 60, 50);
            assert!(near(a, eased(30.5, 40.)), "past the inner's band {a}");
        }
    }
}
