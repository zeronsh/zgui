//! Single-pass presentation (damage drawn into the next retained target and
//! the drawable in one render pass) must put exactly the bytes on screen that
//! the two-pass route does: damage into the retained target, then the present
//! blit into an sRGB, post-multiplied drawable as on macOS.
use zgx::scene::*;
use zgx_gpu::GpuRenderer;

fn fixed(w: f32, h: f32) -> Style {
    Style {
        width: Some(w),
        height: Some(h),
        ..Default::default()
    }
}

/// Builds the same scene in each renderer's scene and applies step `n`.
struct Fixture {
    scene: Scene,
    moving: NodeId,
    fading: NodeId,
    label: NodeId,
    extra: Option<NodeId>,
}
impl Fixture {
    fn new() -> Self {
        let mut scene = Scene::new(240., 160.);
        scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
        scene.append(
            scene.root(),
            NodeKind::Rect(Color(16, 20, 28, 255)),
            fixed(240., 160.),
        );
        for i in 0..6 {
            let card = scene.append(
                scene.root(),
                NodeKind::Panel {
                    layout: Layout::Overlay,
                    quad: QuadStyle {
                        fill: Color(40 + i * 20, 60, 90, 255),
                        radius: 6.,
                        border_color: Color(200, 220, 255, 120),
                        border_width: 1.5,
                        ..Default::default()
                    },
                },
                fixed(60., 30.),
            );
            scene.set_transform(
                card,
                Transform {
                    x: 10. + (i % 3) as f32 * 75.,
                    y: 10. + (i / 3) as f32 * 40.,
                },
            );
        }
        let label = scene.append(
            scene.root(),
            NodeKind::Text {
                text: "frame 0".into(),
                font_size: 14.,
                color: Color(230, 235, 245, 255),
            },
            fixed(200., 20.),
        );
        scene.set_transform(label, Transform { x: 12., y: 100. });
        let moving = scene.append(
            scene.root(),
            NodeKind::Rect(Color(120, 200, 255, 180)),
            fixed(24., 24.),
        );
        let fading = scene.append(
            scene.root(),
            NodeKind::Rect(Color(255, 140, 90, 255)),
            fixed(30., 18.),
        );
        scene.set_transform(fading, Transform { x: 190., y: 120. });
        Self {
            scene,
            moving,
            fading,
            label,
            extra: None,
        }
    }
    fn step(&mut self, n: usize) {
        let f = n as f32;
        self.scene.set_transform(
            self.moving,
            Transform {
                x: 5. + f * 13.7,
                y: 20. + (f * 0.9).sin() * 40.,
            },
        );
        self.scene.set_effects(
            self.fading,
            Effects {
                opacity: (0.15 + 0.1 * f).min(1.),
                ..Default::default()
            },
        );
        self.scene.set_kind(
            self.label,
            NodeKind::Text {
                text: format!("frame {n} · {}", "é→✓".repeat(n % 3)).into(),
                font_size: 14.,
                color: Color(230, 235, 245, 255),
            },
        );
        match (n % 4, self.extra) {
            (1, None) => {
                let extra = self.scene.append(
                    self.scene.root(),
                    NodeKind::Rect(Color(90, 255, 160, 100)),
                    fixed(50., 40.),
                );
                self.scene.set_transform(
                    extra,
                    Transform {
                        x: 150.,
                        y: 60. + f,
                    },
                );
                self.extra = Some(extra);
            }
            (3, Some(extra)) => {
                self.scene.remove(extra);
                self.extra = None;
            }
            _ => {}
        }
    }
}

#[test]
fn single_pass_presentation_matches_the_two_pass_route_exactly() {
    let mut reference = GpuRenderer::new(480, 320).expect("a Metal/Vulkan adapter");
    reference.set_scale_factor(2.);
    let mut single = GpuRenderer::new(480, 320).unwrap();
    single.set_scale_factor(2.);
    // Opaque windows, like every non-transparent zgx window.
    reference.set_background(Color(12, 14, 20, 255));
    single.set_background(Color(12, 14, 20, 255));
    single.debug_enable_single_pass();
    let (mut a, mut b) = (Fixture::new(), Fixture::new());
    for n in 0..16 {
        if n > 0 {
            a.step(n);
            b.step(n);
        }
        let damage = a.scene.flush().damage;
        reference.render(&a.scene, &damage).unwrap();
        let expected = reference.debug_present_offscreen().unwrap();
        let retained = reference.readback().unwrap();
        let damage = b.scene.flush().damage;
        let stats = single.render(&b.scene, &damage).unwrap();
        let presented = single.debug_present_offscreen().unwrap();
        assert_eq!(presented.len(), expected.len());
        let differing = presented
            .iter()
            .zip(&expected)
            .filter(|(p, e)| p != e)
            .count();
        assert_eq!(
            differing, 0,
            "frame {n}: {differing} presented bytes differ"
        );
        assert_eq!(
            single.readback().unwrap(),
            retained,
            "frame {n}: retained target differs"
        );
        if n > 0 {
            assert!(
                stats.render_passes <= 1,
                "frame {n}: {} passes",
                stats.render_passes
            );
        }
        assert_eq!(
            single.debug_single_pass_frames(),
            n as u64 + 1,
            "frame {n} fell back to the two-pass route"
        );
    }
}

#[test]
fn single_pass_matches_every_channel_value_through_the_srgb_drawable() {
    // 256 columns: every byte value in each channel, plain and under a
    // translucent overlay, so no rounding in the sRGB round trip can hide.
    let mut scene = Scene::new(256., 64.);
    scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
    for v in 0..=255_u8 {
        let column = scene.append(
            scene.root(),
            NodeKind::Rect(Color(v, 255 - v, v / 2 + 64, 255)),
            fixed(1., 64.),
        );
        scene.set_transform(
            column,
            Transform {
                x: f32::from(v),
                y: 0.,
            },
        );
    }
    let overlay = scene.append(
        scene.root(),
        NodeKind::Rect(Color(250, 120, 30, 97)),
        fixed(256., 32.),
    );
    scene.set_transform(overlay, Transform { x: 0., y: 32. });
    let mut reference = GpuRenderer::new(256, 64).unwrap();
    let mut single = GpuRenderer::new(256, 64).unwrap();
    single.debug_enable_single_pass();
    for gpu in [&mut reference, &mut single] {
        gpu.set_background(Color(0, 0, 0, 255));
    }
    let damage = scene.flush().damage;
    reference.render(&scene, &damage).unwrap();
    single.render(&scene, &damage).unwrap();
    assert_eq!(
        single.debug_present_offscreen().unwrap(),
        reference.debug_present_offscreen().unwrap()
    );
    assert_eq!(single.debug_single_pass_frames(), 1);
}

#[test]
fn transparent_windows_match_too_where_presentation_keeps_premultiplied_alpha() {
    // Glass windows: a transparent background with translucent panels.
    let mut scene = Scene::new(200., 120.);
    scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
    for i in 0..5 {
        let panel = scene.append(
            scene.root(),
            NodeKind::Panel {
                layout: Layout::Overlay,
                quad: QuadStyle {
                    fill: Color(18 + i * 30, 22, 30, 60 + i * 35),
                    radius: 8.,
                    ..Default::default()
                },
            },
            fixed(90., 60.),
        );
        scene.set_transform(
            panel,
            Transform {
                x: i as f32 * 25.,
                y: i as f32 * 12.,
            },
        );
    }
    let mut reference = GpuRenderer::new(400, 240).unwrap();
    reference.set_scale_factor(2.);
    let mut single = GpuRenderer::new(400, 240).unwrap();
    single.set_scale_factor(2.);
    single.debug_enable_single_pass();
    let damage = scene.flush().damage;
    reference.render(&scene, &damage).unwrap();
    single.render(&scene, &damage).unwrap();
    assert_eq!(
        single.debug_present_offscreen().unwrap(),
        reference.debug_present_offscreen().unwrap()
    );
    // macOS presents premultiplied, so transparent windows take the single pass.
    let expected = u64::from(cfg!(target_os = "macos"));
    assert_eq!(single.debug_single_pass_frames(), expected);
}

#[test]
fn blur_frames_defer_the_tail_after_the_last_filter_and_still_match() {
    // A frosted panel over moving content with a label on top: each frame
    // blurs, then draws the panel's own quads and the label afterwards.
    let build = || {
        let mut f = Fixture::new();
        let glass = f.scene.append(
            f.scene.root(),
            NodeKind::Panel {
                layout: Layout::Overlay,
                quad: QuadStyle {
                    fill: Color(255, 255, 255, 40),
                    radius: 10.,
                    ..Default::default()
                },
            },
            fixed(120., 70.),
        );
        f.scene.set_transform(glass, Transform { x: 40., y: 30. });
        f.scene.set_effects(
            glass,
            Effects {
                blur_radius: 6.,
                ..Default::default()
            },
        );
        let caption = f.scene.append(
            f.scene.root(),
            NodeKind::Text {
                text: "on glass".into(),
                font_size: 12.,
                color: Color(250, 250, 255, 255),
            },
            fixed(100., 16.),
        );
        f.scene.set_transform(caption, Transform { x: 50., y: 60. });
        (f, glass)
    };
    let mut reference = GpuRenderer::new(480, 320).unwrap();
    let mut single = GpuRenderer::new(480, 320).unwrap();
    for gpu in [&mut reference, &mut single] {
        gpu.set_scale_factor(2.);
        gpu.set_background(Color(12, 14, 20, 255));
    }
    single.debug_enable_single_pass();
    let ((mut a, glass_a), (mut b, glass_b)) = (build(), build());
    let mut blurred = 0;
    for n in 0..12 {
        if n > 0 {
            a.step(n);
            b.step(n);
            // Fade the glass as a reveal would.
            for (f, glass) in [(&mut a, glass_a), (&mut b, glass_b)] {
                f.scene.set_effects(
                    glass,
                    Effects {
                        blur_radius: 6.,
                        opacity: (n as f32 / 8.).min(1.),
                        ..Default::default()
                    },
                );
            }
        }
        let damage = a.scene.flush().damage;
        reference.render(&a.scene, &damage).unwrap();
        let expected = reference.debug_present_offscreen().unwrap();
        let retained = reference.readback().unwrap();
        let damage = b.scene.flush().damage;
        let stats = single.render(&b.scene, &damage).unwrap();
        blurred += stats.blur_passes;
        if n > 0 {
            // Draws up to the filter, its three blur passes, then
            // presentation with the blur drawn back and everything after it.
            assert!(
                stats.render_passes <= 5,
                "frame {n}: {} passes",
                stats.render_passes
            );
        }
        // Presented in one pass, the blur is mixed over a copy of the area in
        // the shader; otherwise by the GPU's dual-source blending. Mid-fade
        // the two round a byte apart at most.
        let close = |a: &[u8], b: &[u8]| {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| a.abs_diff(*b) <= 1)
        };
        assert!(
            close(&single.debug_present_offscreen().unwrap(), &expected),
            "frame {n}: presented bytes differ"
        );
        assert!(
            close(&single.readback().unwrap(), &retained),
            "frame {n}: retained target differs"
        );
        assert_eq!(
            single.debug_single_pass_frames(),
            n as u64 + 1,
            "frame {n} fell back to the two-pass route"
        );
    }
    assert!(blurred >= 11, "the moving content must damage the glass");
}

#[test]
fn whole_window_frames_skip_the_retained_target_and_still_match() {
    // A window repainted whole frame after frame (a live dashboard) is drawn
    // into the drawable alone; a partial frame after that repaints everything
    // to bring the retained target back. Every presented frame must match
    // the two-pass route.
    fn scene() -> (Scene, NodeId, NodeId) {
        let mut scene = Scene::new(240., 160.);
        scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
        let background = scene.append(
            scene.root(),
            NodeKind::Rect(Color(16, 20, 28, 255)),
            fixed(240., 160.),
        );
        let card = scene.append(
            scene.root(),
            NodeKind::Panel {
                layout: Layout::Overlay,
                quad: QuadStyle {
                    fill: Color(60, 90, 140, 255),
                    radius: 6.,
                    ..Default::default()
                },
            },
            fixed(60., 30.),
        );
        scene.set_transform(card, Transform { x: 20., y: 20. });
        (scene, background, card)
    }
    let mut reference = GpuRenderer::new(480, 320).expect("a Metal/Vulkan adapter");
    let mut single = GpuRenderer::new(480, 320).unwrap();
    for renderer in [&mut reference, &mut single] {
        renderer.set_scale_factor(2.);
        renderer.set_background(Color(12, 14, 20, 255));
    }
    single.debug_enable_single_pass();
    let (mut a, a_background, a_card) = scene();
    let (mut b, b_background, b_card) = scene();
    // Whole-window frames, partial ones, whole again, then partial.
    let whole = |n: usize| matches!(n, 1..=5 | 9..=11);
    for n in 0..16_usize {
        for (scene, background, card) in [
            (&mut a, a_background, a_card),
            (&mut b, b_background, b_card),
        ] {
            if whole(n) {
                let v = (n * 9) as u8;
                scene.set_kind(background, NodeKind::Rect(Color(16 + v, 20, 28, 255)));
            } else if n > 0 {
                scene.set_transform(
                    card,
                    Transform {
                        x: 20. + n as f32 * 4.,
                        y: 20.,
                    },
                );
            }
        }
        let damage = a.flush().damage;
        reference.render(&a, &damage).unwrap();
        let expected = reference.debug_present_offscreen().unwrap();
        let damage = b.flush().damage;
        single.render(&b, &damage).unwrap();
        let presented = single.debug_present_offscreen().unwrap();
        let differing = presented
            .iter()
            .zip(&expected)
            .filter(|(p, e)| p != e)
            .count();
        assert_eq!(
            differing, 0,
            "frame {n}: {differing} presented bytes differ"
        );
    }
    // Frames 1-5 (the first frame repaints whole too) and 10-11 follow a
    // whole-window frame.
    assert_eq!(single.debug_direct_frames(), 7);
    // Once a partial frame has repainted it, the retained target is current.
    assert_eq!(single.readback().unwrap(), reference.readback().unwrap());
}
