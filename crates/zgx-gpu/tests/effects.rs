//! Layer effects: a content blur (`Effects::filter_blur`) spreads a cached
//! layer past its bounds, a mask (`Scene::set_mask`) hides it by an image's
//! alpha, and animating either composites again without repainting the
//! layer, on the wgpu path and with Metal directly.
use std::sync::Arc;
use zgx::scene::*;
use zgx_gpu::GpuRenderer;

fn gpu_fixture() -> std::sync::MutexGuard<'static, ()> {
    static DEVICE_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());
    DEVICE_TEST
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
const SIZE: u32 = 200;
const RED: Color = Color(255, 0, 0, 255);
fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let at = ((y * SIZE + x) * 4) as usize;
    pixels[at..at + 4].try_into().unwrap()
}
fn scene() -> Scene {
    let mut scene = Scene::new(SIZE as f32, SIZE as f32);
    scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
    scene
}
/// An opaque `color` rect of `size` at (`x`, `y`).
fn rect(scene: &mut Scene, x: f32, y: f32, size: (f32, f32), color: Color) -> NodeId {
    let id = scene.append(
        scene.root(),
        NodeKind::Rect(color),
        Style {
            width: Some(size.0),
            height: Some(size.1),
            ..Default::default()
        },
    );
    scene.set_transform(id, Transform { x, y });
    id
}
/// Render the flushed frame, then check it against a full repaint.
fn render(gpu: &mut GpuRenderer, scene: &mut Scene) -> (zgx_gpu::GpuStats, Vec<u8>) {
    let damage = scene.flush().damage;
    let stats = gpu.render(scene, &damage).unwrap();
    let pixels = gpu.readback().unwrap();
    gpu.render(scene, &[Rect::new(0., 0., SIZE as f32, SIZE as f32)])
        .unwrap();
    assert_eq!(
        pixels,
        gpu.readback().unwrap(),
        "incremental frame differs from full"
    );
    (stats, pixels)
}
fn renderers() -> impl Iterator<Item = (bool, GpuRenderer)> {
    [true, false].into_iter().map(|direct| {
        let mut gpu = GpuRenderer::new(SIZE, SIZE).unwrap();
        gpu.set_direct_metal(direct);
        (direct, gpu)
    })
}

#[test]
fn content_blur_spreads_past_the_bounds_and_animates_without_repainting() {
    let _gpu_fixture = gpu_fixture();
    for (direct, mut gpu) in renderers() {
        let mut scene = scene();
        let square = rect(&mut scene, 80., 80., (40., 40.), RED);
        let (_, sharp) = render(&mut gpu, &mut scene);
        scene.set_filter_blur(square, 4.);
        let (stats, p) = render(&mut gpu, &mut scene);
        assert_eq!(stats.layer_repaints, 1, "direct={direct}");
        assert_eq!(stats.effect_passes, 2, "direct={direct}");
        // Opaque in the middle, soft at the edge, spilling outside it...
        assert!(pixel(&p, 100, 100)[3] > 250, "{:?}", pixel(&p, 100, 100));
        let edge = pixel(&p, 80, 100)[3];
        assert!((100..160).contains(&edge), "edge alpha {edge}");
        let outside = pixel(&p, 75, 100)[3];
        assert!((5..60).contains(&outside), "outside alpha {outside}");
        assert!(pixel(&p, 100, 125)[3] > 0 && pixel(&p, 125, 100)[3] > 0);
        // ...symmetrically, and no further than three sigma.
        assert!(pixel(&p, 124, 100)[3].abs_diff(pixel(&p, 75, 100)[3]) <= 2);
        assert_eq!(pixel(&p, 66, 100), [0; 4]);
        // Premultiplied red throughout.
        let [r, g, b, a] = pixel(&p, 77, 100);
        assert!((r as i32 - a as i32).abs() <= 1 && g == 0 && b == 0);
        // A new sigma composites again from the same layer pixels, its
        // passes drawn with Metal directly where enabled.
        scene.set_filter_blur(square, 2.);
        let metal = gpu.debug_metal_frames();
        let damage = scene.flush().damage;
        gpu.render(&scene, &damage).unwrap();
        if cfg!(target_os = "macos") {
            assert_eq!(gpu.debug_metal_frames() - metal, u64::from(direct));
        }
        scene.set_filter_blur(square, 3.);
        render(&mut gpu, &mut scene);
        scene.set_filter_blur(square, 2.);
        let (stats, narrower) = render(&mut gpu, &mut scene);
        assert_eq!((stats.layer_repaints, stats.layer_cache_hits), (0, 1));
        assert_eq!(stats.effect_passes, 2);
        assert!(pixel(&narrower, 75, 100)[3] < outside);
        // Idle frames redraw nothing; damage elsewhere leaves the slot be.
        let other = rect(&mut scene, 0., 0., (10., 10.), RED);
        let (stats, _) = render(&mut gpu, &mut scene);
        assert_eq!(stats.effect_passes, 0);
        scene.set_kind(other, NodeKind::Rect(Color(0, 0, 255, 255)));
        // Damage overlapping the blur composites it again, unchanged.
        scene.set_transform(other, Transform { x: 70., y: 95. });
        let (stats, _) = render(&mut gpu, &mut scene);
        assert_eq!((stats.effect_passes, stats.layer_repaints), (0, 0));
        scene.remove(other);
        render(&mut gpu, &mut scene);
        // Back at 0 it draws exactly as it did unblurred.
        scene.set_filter_blur(square, 0.);
        let (stats, p) = render(&mut gpu, &mut scene);
        assert_eq!(stats.effect_passes, 0);
        assert_eq!(p, sharp, "direct={direct}");
        if direct && cfg!(target_os = "macos") {
            assert!(gpu.debug_metal_frames() > 0, "never drew with Metal");
        }
    }
}

#[test]
fn opacity_applies_to_the_blurred_result_and_transforms_compose() {
    let _gpu_fixture = gpu_fixture();
    for (direct, mut gpu) in renderers() {
        let mut scene = scene();
        let square = rect(&mut scene, 80., 80., (40., 40.), RED);
        scene.set_filter_blur(square, 3.);
        let (_, opaque) = render(&mut gpu, &mut scene);
        scene.set_effects(
            square,
            Effects {
                opacity: 0.5,
                filter_blur: 3.,
                ..Effects::default()
            },
        );
        let (stats, half) = render(&mut gpu, &mut scene);
        assert_eq!(stats.layer_repaints, 0);
        for (x, y) in [(100, 100), (80, 100), (74, 100), (100, 116)] {
            let (full, half) = (pixel(&opaque, x, y)[3], pixel(&half, x, y)[3]);
            assert!(
                (half as i32 - (full as i32 + 1) / 2).abs() <= 1,
                "direct={direct} ({x}, {y}): {full} → {half}"
            );
        }
        // Scaled twice about its centre, the blur scales with it: the soft
        // edge lands 20 px out, as wide as twice the sigma.
        scene.set_effects(
            square,
            Effects {
                filter_blur: 3.,
                ..Effects::default()
            },
        );
        scene.set_local_transform(
            square,
            LocalTransform {
                scale: (2., 2.),
                ..LocalTransform::IDENTITY
            },
        );
        let (_, p) = render(&mut gpu, &mut scene);
        let edge = pixel(&p, 60, 100)[3];
        assert!((100..160).contains(&edge), "scaled edge alpha {edge}");
        assert!(pixel(&p, 52, 100)[3] > 0);
        assert_eq!(pixel(&p, 40, 100), [0; 4]);
    }
}

#[test]
fn wide_blurs_and_many_blurred_words_share_two_passes() {
    let _gpu_fixture = gpu_fixture();
    for (direct, mut gpu) in renderers() {
        let mut scene = scene();
        let words: Vec<NodeId> = (0..24)
            .map(|i| {
                let (x, y) = ((i % 6) as f32 * 30. + 10., (i / 6) as f32 * 40. + 20.);
                rect(&mut scene, x, y, (20., 10.), RED)
            })
            .collect();
        render(&mut gpu, &mut scene);
        for step in 1..=4 {
            for word in &words {
                scene.set_effects(
                    *word,
                    Effects {
                        opacity: step as f32 / 4.,
                        filter_blur: 2. / step as f32,
                        ..Effects::default()
                    },
                );
            }
            let (stats, p) = render(&mut gpu, &mut scene);
            assert_eq!(stats.effect_passes, 2, "direct={direct} step {step}");
            assert!(pixel(&p, 20, 25)[3] > 0);
        }
        // A wide blur averages down: still Gaussian, opaque over three
        // sigma in, half at the edge, smooth, reaching three sigma out.
        let mut scene = self::scene();
        let panel = rect(&mut scene, 60., 60., (80., 80.), RED);
        scene.set_filter_blur(panel, 12.);
        let (_, p) = render(&mut gpu, &mut scene);
        let centre = pixel(&p, 100, 100)[3];
        assert!(centre >= 252, "centre alpha {centre}");
        let edge = pixel(&p, 60, 100)[3];
        assert!((115..140).contains(&edge), "edge alpha {edge}");
        assert!(pixel(&p, 40, 100)[3] > 0);
        assert_eq!(pixel(&p, 20, 100), [0; 4]);
        let row: Vec<u8> = (30..70).map(|x| pixel(&p, x, 100)[3]).collect();
        assert!(
            row.windows(2).all(|w| w[0] <= w[1] && w[1] - w[0] <= 12),
            "{row:?}"
        );
    }
}

#[test]
fn blurs_nest_inside_layers_and_blurred_rows() {
    let _gpu_fixture = gpu_fixture();
    for (direct, mut gpu) in renderers() {
        let mut scene = scene();
        let row = scene.append(
            scene.root(),
            NodeKind::Container(Layout::Overlay),
            Style {
                width: Some(100.),
                height: Some(40.),
                ..Default::default()
            },
        );
        scene.set_transform(row, Transform { x: 50., y: 80. });
        let word = scene.append(
            row,
            NodeKind::Rect(RED),
            Style {
                width: Some(20.),
                height: Some(20.),
                ..Default::default()
            },
        );
        scene.set_transform(word, Transform { x: 40., y: 0. });
        // A word fading in inside a row faded as a group.
        scene.set_isolated(row, true);
        scene.set_effects(
            row,
            Effects {
                opacity: 0.999,
                ..Effects::default()
            },
        );
        scene.set_filter_blur(word, 3.);
        let (stats, p) = render(&mut gpu, &mut scene);
        assert_eq!(
            (stats.layer_repaints, stats.effect_passes),
            (2, 2),
            "direct={direct}"
        );
        // The word's blur reaches past it, and past the row's own box.
        assert!(pixel(&p, 87, 90)[3] > 0);
        assert!(pixel(&p, 100, 76)[3] > 0);
        // Changing the word's sigma repaints the row's texture, not the
        // word's, and the row composites as it is.
        scene.set_filter_blur(word, 1.);
        let (stats, _) = render(&mut gpu, &mut scene);
        assert_eq!((stats.layer_repaints, stats.layer_cache_hits), (1, 1));
        // The whole row crossfading with a blur of its own on top.
        scene.set_effects(
            row,
            Effects {
                opacity: 0.5,
                filter_blur: 4.,
                ..Effects::default()
            },
        );
        let (stats, p) = render(&mut gpu, &mut scene);
        assert_eq!(stats.layer_repaints, 0);
        let centre = pixel(&p, 100, 90)[3];
        assert!(centre < 128 && centre > 60, "centre alpha {centre}");
        assert!(pixel(&p, 82, 90)[3] > 0);
        assert_eq!(pixel(&p, 60, 90), [0; 4]);
    }
}

/// A `width` × 1 mask image whose texels' alphas are `alphas`.
fn mask_image(alphas: &[u8]) -> Arc<zgx::image::ImageData> {
    let pixels: Vec<u8> = alphas.iter().flat_map(|a| [255, 255, 255, *a]).collect();
    Arc::new(zgx::image::ImageData::new(alphas.len() as u32, 1, pixels).unwrap())
}

#[test]
fn masks_hide_by_alpha_and_slide_without_repainting() {
    let _gpu_fixture = gpu_fixture();
    for (direct, mut gpu) in renderers() {
        let mut scene = scene();
        let label = rect(&mut scene, 80., 80., (40., 40.), RED);
        render(&mut gpu, &mut scene);
        // A band 10 px wide: opaque, half and clear texels.
        scene.set_mask(
            label,
            Some(ImageMask {
                image: mask_image(&[255, 128, 0, 0]),
                offset: (0., 0.),
                size: (10., 40.),
            }),
        );
        let (stats, p) = render(&mut gpu, &mut scene);
        assert_eq!(stats.layer_repaints, 1, "direct={direct}");
        assert_eq!(stats.effect_passes, 1);
        assert_eq!(pixel(&p, 81, 100), [255, 0, 0, 255]);
        let half = pixel(&p, 83, 100);
        assert!(half[3].abs_diff(128) <= 1 && half[0] == half[3], "{half:?}");
        // Nearest: no ramp between texels; past the image, nothing.
        assert_eq!(pixel(&p, 86, 100), [0; 4]);
        assert_eq!(pixel(&p, 100, 100), [0; 4]);
        // Sliding the band composites again from the same layer.
        scene.set_mask_offset(label, (20., 0.));
        let (stats, p) = render(&mut gpu, &mut scene);
        assert_eq!((stats.layer_repaints, stats.layer_cache_hits), (0, 1));
        assert_eq!(stats.effect_passes, 1);
        assert_eq!(pixel(&p, 81, 100), [0; 4]);
        assert_eq!(pixel(&p, 101, 100), [255, 0, 0, 255]);
        // Blurred first, then masked, then faded.
        scene.set_effects(
            label,
            Effects {
                opacity: 0.5,
                filter_blur: 2.,
                ..Effects::default()
            },
        );
        let (stats, p) = render(&mut gpu, &mut scene);
        assert_eq!((stats.layer_repaints, stats.effect_passes), (0, 2));
        let inside = pixel(&p, 101, 100)[3];
        assert!((100..=128).contains(&inside), "masked blur {inside}");
        // The mask cuts the blur's spread above the box too.
        assert_eq!(pixel(&p, 101, 77), [0; 4]);
        assert_eq!(pixel(&p, 90, 100), [0; 4]);
        // Unmasked and unblurred, the label shows whole again.
        scene.set_effects(label, Effects::default());
        scene.set_mask(label, None);
        let (_, p) = render(&mut gpu, &mut scene);
        assert_eq!(pixel(&p, 81, 100), [255, 0, 0, 255]);
        assert_eq!(pixel(&p, 118, 118), [255, 0, 0, 255]);
    }
}

#[test]
fn blurs_match_a_gaussian_at_every_width() {
    let _gpu_fixture = gpu_fixture();
    // Full size up to sigma 4 px, averaged down (to an eighth at 30) past.
    for sigma in [1., 3., 4., 12., 30.] {
        let mut gpu = GpuRenderer::new(SIZE, SIZE).unwrap();
        let mut scene = scene();
        let panel = rect(&mut scene, 60., 60., (80., 80.), RED);
        scene.set_filter_blur(panel, sigma);
        let (_, p) = render(&mut gpu, &mut scene);
        // The 80 px square convolved with the Gaussian, along its middle row.
        let phi = |x: f32| 0.5 * (1. + erf(x / std::f32::consts::SQRT_2));
        let across = phi(40. / sigma) - phi(-40. / sigma);
        for x in 0..SIZE {
            let at = x as f32 + 0.5;
            let expected = 255. * (phi((at - 60.) / sigma) - phi((at - 140.) / sigma)) * across;
            let alpha = pixel(&p, x, 100)[3] as f32;
            assert!(
                (alpha - expected).abs() <= 3.,
                "sigma {sigma} at x {x}: {alpha} for {expected}"
            );
        }
    }
}
/// Abramowitz and Stegun 7.1.26: within 1.5e-7.
fn erf(x: f32) -> f32 {
    let t = 1. / (1. + 0.327_591_1 * x.abs());
    let poly = ((((1.061_405_4 * t - 1.453_152_1) * t + 1.421_413_8) * t - 0.284_496_74) * t
        + 0.254_829_6)
        * t;
    (1. - poly * (-x * x).exp()).copysign(x)
}
