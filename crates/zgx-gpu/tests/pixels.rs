use zgx::scene::*;
use zgx_gpu::GpuRenderer;
// These tests create independent devices on the same adapter. Keep their resource
// lifetimes separate rather than multiplying driver threads/caches by the test
// runner's CPU count. Tests that exercise shared devices still create multiple
// renderers within one fixture.
fn gpu_fixture() -> std::sync::MutexGuard<'static, ()> {
    static DEVICE_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());
    DEVICE_TEST
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
fn fixed(w: f32, h: f32) -> Style {
    Style {
        width: Some(w),
        height: Some(h),
        ..Default::default()
    }
}
#[test]
fn retained_damage_alpha_text_and_scale() {
    let _gpu_fixture = gpu_fixture();
    let mut gpu = GpuRenderer::new(128, 64).expect("a Vulkan/Metal adapter is required");
    let mut s = Scene::new(128., 64.);
    s.set_kind(s.root(), NodeKind::Container(Layout::Overlay));
    let bg = s.append(
        s.root(),
        NodeKind::Rect(Color(20, 40, 60, 255)),
        fixed(128., 64.),
    );
    let front = s.append(
        s.root(),
        NodeKind::Rect(Color(200, 0, 0, 128)),
        fixed(20., 20.),
    );
    let damage = s.flush().damage;
    let stats = gpu.render(&s, &damage).unwrap();
    assert!(stats.draw_calls > 0);
    let pixels = gpu.readback().unwrap();
    assert_eq!(
        gpu.present_with_status().unwrap(),
        zgx_gpu::PresentationStatus::Offscreen
    );
    gpu.present().unwrap();
    assert_eq!(pixels, gpu.readback().unwrap());
    assert_eq!(
        &pixels[(30 * 128 + 30) * 4..(30 * 128 + 30) * 4 + 4],
        &[20, 40, 60, 255]
    );
    assert!((pixels[0] as i32 - 110).abs() <= 1);
    assert_eq!(gpu.render(&s, &[]).unwrap().draw_calls, 0);
    s.set_transform(front, Transform { x: 30., y: 0. });
    {
        let d = s.flush().damage;
        gpu.render(&s, &d).unwrap()
    };
    let partial = gpu.readback().unwrap();
    gpu.render(&s, &[Rect::new(0., 0., 128., 64.)]).unwrap();
    assert_eq!(partial, gpu.readback().unwrap());
    assert_eq!(&partial[..4], &[20, 40, 60, 255]);
    let text = s.append(
        s.root(),
        NodeKind::Text {
            text: "Hello العربية".into(),
            color: Color(255, 255, 255, 255),
            font_size: 16.,
        },
        fixed(128., 32.),
    );
    s.set_transform(text, Transform { x: 0., y: 25. });
    let d = s.flush().damage;
    let stats = gpu.render(&s, &d).unwrap();
    assert_eq!(stats.shaped_nodes, 1);
    assert!(stats.glyph_uploads > 0);
    let pixels = gpu.readback().unwrap();
    assert!(
        pixels
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[0] > 200 && p[1] > 200)
    );
    assert_eq!(gpu.render(&s, &d).unwrap().shaped_nodes, 0);
    s.set_effects(
        front,
        Effects {
            edge_fade: 10.,
            ..Default::default()
        },
    );
    let d = s.flush().damage;
    gpu.render(&s, &d).unwrap();
    let partial = gpu.readback().unwrap();
    gpu.render(&s, &[Rect::new(0., 0., 128., 64.)]).unwrap();
    assert_eq!(partial, gpu.readback().unwrap());
    s.remove(text);
    s.remove(front);
    s.set_kind(bg, NodeKind::Rect(Color(1, 2, 3, 255)));
    s.resize(64., 32.);
    s.set_style(bg, fixed(64., 32.));
    gpu.set_scale_factor(2.);
    {
        let d = s.flush().damage;
        gpu.render(&s, &d).unwrap()
    };
    assert_eq!(&gpu.readback().unwrap()[..4], &[1, 2, 3, 255]);
}

#[test]
fn backdrop_blur_reconstructs_without_feedback() {
    let _gpu_fixture = gpu_fixture();
    let mut gpu = GpuRenderer::new(64, 32).unwrap();
    let mut s = Scene::new(64., 32.);
    s.set_kind(s.root(), NodeKind::Container(Layout::Overlay));
    s.append(
        s.root(),
        NodeKind::Rect(Color(0, 0, 0, 255)),
        fixed(64., 32.),
    );
    let white = s.append(
        s.root(),
        NodeKind::Rect(Color(255, 255, 255, 255)),
        fixed(32., 32.),
    );
    let glass = s.append(
        s.root(),
        NodeKind::Rect(Color(255, 0, 0, 40)),
        fixed(64., 32.),
    );
    s.set_effects(
        glass,
        Effects {
            blur_radius: 4.,
            ..Default::default()
        },
    );
    let d = s.flush().damage;
    gpu.render(&s, &d).unwrap();
    let first = gpu.readback().unwrap();
    let index = (16 * 64 + 32) * 4;
    assert!(
        first[index + 1] > 20 && first[index + 1] < 200,
        "blur crosses the white/black boundary"
    );
    gpu.render(&s, &d).unwrap();
    assert_eq!(first, gpu.readback().unwrap(), "no repeated-blur feedback");
    s.set_kind(white, NodeKind::Rect(Color(0, 255, 0, 255)));
    let d = s.flush().damage;
    gpu.render(&s, &d).unwrap();
    let partial = gpu.readback().unwrap();
    gpu.render(&s, &[Rect::new(0., 0., 64., 32.)]).unwrap();
    assert_eq!(partial, gpu.readback().unwrap());
}

#[test]
fn rounded_backdrop_blur_keeps_pixels_outside_its_corners() {
    let _gpu_fixture = gpu_fixture();
    let mut gpu = GpuRenderer::new(64, 64).unwrap();
    let mut s = Scene::new(64., 64.);
    s.set_kind(s.root(), NodeKind::Container(Layout::Overlay));
    s.append(
        s.root(),
        NodeKind::Rect(Color(0, 0, 0, 255)),
        fixed(64., 64.),
    );
    s.append(
        s.root(),
        NodeKind::Rect(Color(255, 255, 255, 255)),
        fixed(4., 64.),
    );
    let glass = s.append(
        s.root(),
        NodeKind::Quad(QuadStyle {
            radius: 20.,
            ..Default::default()
        }),
        fixed(64., 64.),
    );
    s.set_effects(
        glass,
        Effects {
            blur_radius: 4.,
            ..Default::default()
        },
    );
    let d = s.flush().damage;
    gpu.render(&s, &d).unwrap();
    let pixels = gpu.readback().unwrap();
    let at = |x: usize, y: usize| pixels[(y * 64 + x) * 4];
    assert_eq!(at(1, 1), 255, "outside the corner curve stays sharp");
    assert!(at(1, 32) < 240, "inside the curve is blurred");
    assert!(at(8, 32) > 10, "the stripe spreads into the panel");
}

#[test]
fn rounded_border_shadow_images_and_background_damage() {
    let _gpu_fixture = gpu_fixture();
    let mut gpu = GpuRenderer::new(80, 60).unwrap();
    gpu.set_background(Color(0, 0, 0, 0));
    let mut scene = Scene::new(80., 60.);
    scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
    let id = scene.append(
        scene.root(),
        NodeKind::Quad(QuadStyle {
            decoration: None,
            fill: Color(255, 0, 0, 255),
            radius: 8.,
            border_color: Color(0, 255, 0, 255),
            border_width: 3.,
            shadow: Some(BoxShadow {
                color: Color(0, 0, 255, 128),
                offset: Transform { x: -6., y: 2. },
                blur_radius: 3.,
                spread: 1.,
            }),
            border_over: false,
            outline: None,
        }),
        fixed(24., 24.),
    );
    scene.set_transform(id, Transform { x: 20., y: 10. });
    let d = scene.flush().damage;
    assert!(d.iter().any(|r| r.x <= 4.));
    gpu.render(&scene, &d).unwrap();
    let pixels = gpu.readback().unwrap();
    let pixel = |x: usize, y: usize| &pixels[(y * 80 + x) * 4..(y * 80 + x) * 4 + 4];
    assert_eq!(pixel(32, 22), &[255, 0, 0, 255]);
    assert!(pixel(32, 10)[1] > 200);
    assert!(pixel(20, 10)[0] < 20);
    assert!(pixel(13, 22)[2] > 0);
    scene.set_transform(id, Transform { x: 40., y: 20. });
    let d = scene.flush().damage;
    gpu.render(&scene, &d).unwrap();
    let partial = gpu.readback().unwrap();
    gpu.render(&scene, &[Rect::new(0., 0., 80., 60.)]).unwrap();
    assert_eq!(partial, gpu.readback().unwrap());
    let image = std::sync::Arc::new(
        zgx::image::ImageData::new(
            2,
            2,
            vec![
                255, 255, 0, 255, 255, 255, 0, 255, 255, 255, 0, 255, 255, 255, 0, 255,
            ],
        )
        .unwrap(),
    );
    let group = scene.append(
        scene.root(),
        NodeKind::Container(Layout::Overlay),
        Style {
            clip: true,
            ..fixed(6., 6.)
        },
    );
    scene.append(group, NodeKind::Image(image), fixed(16., 16.));
    let d = scene.flush().damage;
    gpu.render(&scene, &d).unwrap();
    let pixels = gpu.readback().unwrap();
    assert_eq!(&pixels[..4], &[255, 255, 0, 255]);
    assert_eq!(&pixels[7 * 4..8 * 4], &[0, 0, 0, 0]);
    scene.remove(group);
    scene.remove(id);
    let d = scene.flush().damage;
    gpu.render(&scene, &d).unwrap();
    assert!(gpu.readback().unwrap().iter().all(|b| *b == 0));
    gpu.set_background(Color(10, 20, 30, 128));
    gpu.render(&scene, &[]).unwrap();
    let p = gpu.readback().unwrap();
    assert_eq!(&p[..4], &[5, 10, 15, 128]);
}

#[cfg(all(feature = "svg", feature = "png"))]
#[test]
fn image_and_svg_decoding_are_bounded_and_alpha_correct() {
    let _gpu_fixture = gpu_fixture();
    let svg=br#"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="2"><rect width="2" height="2" fill="red" fill-opacity="0.5"/></svg>"#;
    let image = zgx_gpu::assets::decode_svg(svg, 2, 2).unwrap();
    assert_eq!(&image.pixels()[..4], &[255, 0, 0, 128]);
    assert!(zgx_gpu::assets::decode_svg(svg, 0, 2).is_err());
    assert!(zgx_gpu::assets::decode_image(b"invalid").is_err());
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        2,
        2,
        image::Rgba([12, 34, 56, 128]),
    ))
    .write_to(&mut png, image::ImageFormat::Png)
    .unwrap();
    let decoded = zgx_gpu::assets::decode_image(png.get_ref()).unwrap();
    assert_eq!(&decoded.pixels()[..4], &[12, 34, 56, 128]);
    // A raster sent where an SVG was asked for draws as the raster, scaled.
    let raster = zgx_gpu::assets::decode_svg(png.get_ref(), 4, 4).unwrap();
    assert_eq!((raster.width(), raster.height()), (4, 4));
    let middle = &raster.pixels()[(4 + 1) * 4..][..4];
    assert!(middle[0].abs_diff(12) <= 2 && middle[1].abs_diff(34) <= 2);
    assert!(middle[2].abs_diff(56) <= 2 && middle[3].abs_diff(128) <= 2);
}

#[test]
fn text_metrics_match_editor_line_pitch() {
    let _gpu_fixture = gpu_fixture();
    let mut fonts = cosmic_text::FontSystem::new();
    assert_eq!(zgx_gpu::measure_text(&mut fonts, "", 14., None).1, 20.);
    assert_eq!(zgx_gpu::measure_text(&mut fonts, "a\nb", 14., None).1, 40.);
}

#[test]
fn thousand_streaming_frames_keep_caches_bounded_and_reuse_vertices() {
    let _gpu_fixture = gpu_fixture();
    let mut gpu = GpuRenderer::new(160, 80).unwrap();
    let mut scene = Scene::new(160., 80.);
    scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
    let stable = scene.append(
        scene.root(),
        NodeKind::Text {
            text: "Retained العربية".into(),
            font_size: 14.,
            color: Color(255, 255, 255, 255),
        },
        fixed(160., 24.),
    );
    let mut changing = scene.append(
        scene.root(),
        NodeKind::Text {
            text: "frame 0".into(),
            font_size: 14.,
            color: Color(255, 0, 0, 255),
        },
        fixed(160., 24.),
    );
    scene.set_transform(changing, Transform { x: 0., y: 30. });
    let d = scene.flush().damage;
    gpu.render(&scene, &d).unwrap();
    // Unified-memory adapters fill a small ring of CPU-written vertex buffers
    // while earlier frames are still in flight; reuse starts once it is warm.
    for x in [2., 3., 2., 3.] {
        scene.set_transform(stable, Transform { x, y: 0. });
        let d = scene.flush().damage;
        gpu.render(&scene, &d).unwrap();
    }
    let initial = gpu.debug_cache_stats().vertex_buffer_bytes;
    // Vertices written in place (unified memory, Direct3D 12) go round a
    // ring of up to four buffers in flight; a staging copy keeps one. Each
    // stays within the one buffer's bound.
    let buffer = initial / gpu.debug_cache_stats().vertex_buffers.max(1);
    scene.set_transform(stable, Transform { x: 1., y: 0. });
    let d = scene.flush().damage;
    let stats = gpu.render(&scene, &d).unwrap();
    assert_eq!(stats.geometry_rebuilds, 0);
    assert_eq!(stats.glyph_uploads, 0);
    assert_eq!(stats.vertex_buffer_allocations, 0);
    for frame in 0..1000 {
        if frame % 20 == 0 {
            scene.remove(changing);
            changing = scene.append(
                scene.root(),
                NodeKind::Text {
                    text: "reused".into(),
                    font_size: 14.,
                    color: Color(255, 0, 0, 255),
                },
                fixed(160., 24.),
            );
            scene.set_transform(changing, Transform { x: 0., y: 30. });
        }
        scene.set_kind(
            changing,
            NodeKind::Text {
                text: format!(
                    "frame {frame} {}",
                    char::from_u32(0x400 + (frame % 80)).unwrap()
                )
                .into(),
                font_size: 12. + (frame % 20) as f32 * 0.25,
                color: Color(255, 0, 0, 255),
            },
        );
        let d = scene.flush().damage;
        gpu.render(&scene, &d).unwrap();
        let c = gpu.debug_cache_stats();
        assert!(c.shaped_nodes <= 2);
        assert!(c.shaped_bytes <= 8 * 1024 * 1024);
        assert_eq!(c.swash_images, 0);
        assert_eq!(c.swash_outlines, 0);
        assert!(
            c.vertex_buffers <= 4,
            "frame {frame}: {} vertex buffers",
            c.vertex_buffers
        );
        assert!(
            c.vertex_buffer_bytes <= c.vertex_buffers * buffer.max(8192),
            "frame {frame}: {} bytes in {} buffers",
            c.vertex_buffer_bytes,
            c.vertex_buffers
        );
        assert!(c.atlas_entries < 5000);
    }
    gpu.readback().unwrap();
    scene.remove(changing);
    scene.remove(stable);
    let d = scene.flush().damage;
    gpu.render(&scene, &d).unwrap();
    assert_eq!(gpu.debug_cache_stats().shaped_nodes, 0);
}

#[test]
fn frames_in_flight_never_share_a_vertex_buffer_being_read() {
    let _gpu_fixture = gpu_fixture();
    // Frames submitted back to back, never waited for, go round the ring of
    // vertex buffers many times while earlier ones are still in flight. Each
    // draws one new mark and leaves the others: a buffer rewritten before
    // the GPU read it would draw an earlier frame's mark where a later one
    // goes, and leave its own place empty.
    let mut gpu = GpuRenderer::new(256, 64).unwrap();
    let mut scene = Scene::new(256., 64.);
    scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
    scene.append(
        scene.root(),
        NodeKind::Rect(Color(0, 0, 0, 255)),
        fixed(256., 64.),
    );
    let d = scene.flush().damage;
    gpu.render(&scene, &d).unwrap();
    let place = |frame: u32| ((frame % 32) * 8, (frame / 32) * 32);
    for frame in 0..64 {
        let mark = scene.append(
            scene.root(),
            NodeKind::Rect(Color(255, frame as u8 * 4, 0, 255)),
            fixed(4., 4.),
        );
        let (x, y) = place(frame);
        scene.set_transform(
            mark,
            Transform {
                x: x as f32,
                y: y as f32,
            },
        );
        let d = scene.flush().damage;
        gpu.render(&scene, &d).unwrap();
        assert!(gpu.debug_cache_stats().vertex_buffers <= 4);
    }
    let pixels = gpu.readback().unwrap();
    for frame in 0..64 {
        let (x, y) = place(frame);
        let at = (((y + 2) * 256 + x + 2) * 4) as usize;
        assert_eq!(
            &pixels[at..at + 4],
            &[255, frame as u8 * 4, 0, 255],
            "frame {frame}'s mark"
        );
    }
}

#[test]
fn shaped_hit_caret_and_disjoint_bidi_selection_use_visual_clusters() {
    let _gpu_fixture = gpu_fixture();
    use zgx_gpu::text::{Affinity, ShapedText, TextCursor};
    let mut fonts = cosmic_text::FontSystem::new();
    let rtl = ShapedText::new(&mut fonts, "אבג", 18., None);
    let begin = rtl
        .caret(TextCursor {
            byte_index: 0,
            affinity: Affinity::After,
        })
        .unwrap();
    let end = rtl
        .caret(TextCursor {
            byte_index: 6,
            affinity: Affinity::Before,
        })
        .unwrap();
    assert!(begin.x > end.x);
    assert_eq!(rtl.hit_test(end.x + 0.1, 10.).unwrap().byte_index, 6);
    assert_eq!(rtl.hit_test(begin.x - 0.1, 10.).unwrap().byte_index, 0);
    let mixed = ShapedText::new(&mut fonts, "abc אבג xyz", 18., None);
    let spans = mixed.selection(2..6);
    assert_eq!(spans.len(), 2);
    assert!(spans[0].x + spans[0].width < spans[1].x);
    let ligature = ShapedText::new(&mut fonts, "office", 18., None);
    let positions: Vec<_> = (1..5)
        .map(|byte_index| {
            ligature
                .caret(TextCursor {
                    byte_index,
                    affinity: Affinity::After,
                })
                .unwrap()
                .x
        })
        .collect();
    assert!(positions.windows(2).all(|w| w[0] < w[1]));
    let combining = ShapedText::new(&mut fonts, "e\u{301}x", 18., None);
    for x in 0..40 {
        let cursor = combining.hit_test(x as f32, 5.).unwrap();
        assert!([0, 3, 4].contains(&cursor.byte_index));
    }
    let lines = ShapedText::new(&mut fonts, "a\r\nb", 14., None);
    assert_eq!(lines.hit_test(0., 25.).unwrap().byte_index, 3);
    assert_eq!(
        lines
            .caret(TextCursor {
                byte_index: 3,
                affinity: Affinity::After
            })
            .unwrap()
            .y,
        20.
    );
}

#[test]
fn scene_installs_native_text_geometry() {
    let _gpu_fixture = gpu_fixture();
    let fonts = std::rc::Rc::new(std::cell::RefCell::new(cosmic_text::FontSystem::new()));
    let mut scene = Scene::new(100., 100.);
    scene.set_text_shaper(
        move |text: &str, size: f32, width: Option<f32>| -> Box<dyn zgx::text_layout::TextLayout> {
            Box::new(zgx_gpu::text::ShapedText::new(
                &mut fonts.borrow_mut(),
                text,
                size,
                width,
            ))
        },
    );
    let layout = scene.shape_text("אבג", 14., None);
    assert!(layout.caret(0).x > layout.caret(6).x);
    assert_eq!(scene.measure_text("a\nb", 14., None).1, 40.);
}

#[test]
fn image_cache_reclaims_removed_nodes_and_leaves_out_oversized_allocations() {
    let _gpu_fixture = gpu_fixture();
    let mut gpu = GpuRenderer::new(8, 8).unwrap();
    let mut scene = Scene::new(8., 8.);
    let image = std::sync::Arc::new(zgx::image::ImageData::new(2, 2, vec![255; 16]).unwrap());
    let node = scene.append(scene.root(), NodeKind::Image(image.clone()), fixed(8., 8.));
    let d = scene.flush().damage;
    gpu.render(&scene, &d).unwrap();
    assert_eq!(gpu.debug_cache_stats().image_textures, 1);
    assert_eq!(gpu.debug_cache_stats().image_bytes, 16);
    scene.remove(node);
    let d = scene.flush().damage;
    gpu.render(&scene, &d).unwrap();
    assert_eq!(gpu.debug_cache_stats().image_textures, 0);
    let huge = std::sync::Arc::new(
        zgx::image::ImageData::new(4096, 4097, vec![0; 4096 * 4097 * 4]).unwrap(),
    );
    let hidden = scene.append(scene.root(), NodeKind::Image(huge), fixed(8., 8.));
    scene.set_effects(
        hidden,
        Effects {
            opacity: 0.,
            ..Default::default()
        },
    );
    let d = scene.flush().damage;
    assert_eq!(gpu.render(&scene, &d).unwrap().image_uploads, 0);
    assert_eq!(gpu.debug_cache_stats().image_bytes, 0);
    scene.set_effects(hidden, Effects::default());
    let d = scene.flush().damage;
    gpu.render(&scene, &d).unwrap();
    // Shown in 8 by 8: held at 8 by 9 (its halving at least that), not
    // 4096 by 4097.
    assert!(gpu.take_issues().is_empty());
    assert_eq!(gpu.debug_cache_stats().image_bytes, 8 * 9 * 4);
}

#[test]
fn isolated_opacity_composites_once_and_retains_transformed_subtrees() {
    let _gpu_fixture = gpu_fixture();
    let mut gpu = GpuRenderer::new(80, 40).unwrap();
    let mut scene = Scene::new(80., 40.);
    scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
    let group = scene.append(
        scene.root(),
        NodeKind::Container(Layout::Overlay),
        fixed(24., 24.),
    );
    scene.set_isolated(group, true);
    scene.set_effects(
        group,
        Effects {
            opacity: 0.5,
            ..Default::default()
        },
    );
    scene.append(
        group,
        NodeKind::Rect(Color(255, 0, 0, 255)),
        fixed(24., 24.),
    );
    let top = scene.append(
        group,
        NodeKind::Rect(Color(0, 0, 255, 255)),
        fixed(16., 24.),
    );
    scene.set_transform(top, Transform { x: 8., y: 0. });
    let d = scene.flush().damage;
    let stats = gpu.render(&scene, &d).unwrap();
    assert_eq!(stats.layer_repaints, 1);
    let p = gpu.readback().unwrap();
    // Half of 255 rounds either way, by GPU.
    let half = &p[(10 * 80 + 12) * 4..(10 * 80 + 12) * 4 + 4];
    assert!(near(half, &[0, 0, 128, 128]), "{half:?}");
    assert!(near(
        &p[(10 * 80 + 2) * 4..(10 * 80 + 2) * 4 + 4],
        &[128, 0, 0, 128]
    ));
    scene.set_transform(group, Transform { x: 30., y: 5. });
    scene.set_effects(
        group,
        Effects {
            opacity: 0.25,
            ..Default::default()
        },
    );
    let d = scene.flush().damage;
    let stats = gpu.render(&scene, &d).unwrap();
    assert_eq!(stats.layer_repaints, 0);
    assert_eq!(stats.layer_cache_hits, 1);
    let p = gpu.readback().unwrap();
    assert!(near(
        &p[(10 * 80 + 42) * 4..(10 * 80 + 42) * 4 + 4],
        &[0, 0, 64, 64]
    ));
    assert_eq!(&p[..4], &[0, 0, 0, 0]);
    scene.set_kind(top, NodeKind::Rect(Color(0, 255, 0, 255)));
    let d = scene.flush().damage;
    assert_eq!(gpu.render(&scene, &d).unwrap().layer_repaints, 1);
    let partial = gpu.readback().unwrap();
    gpu.render(&scene, &[Rect::new(0., 0., 80., 40.)]).unwrap();
    assert_eq!(partial, gpu.readback().unwrap());
    scene.remove(group);
    let d = scene.flush().damage;
    gpu.render(&scene, &d).unwrap();
    assert_eq!(gpu.debug_cache_stats().layer_textures, 0);
    assert!(gpu.readback().unwrap().iter().all(|b| *b == 0));
}

#[test]
fn nested_layers_apply_opacity_at_each_boundary_and_invalidate_ancestors() {
    let _gpu_fixture = gpu_fixture();
    let mut gpu = GpuRenderer::new(64, 40).unwrap();
    let mut scene = Scene::new(64., 40.);
    let outer = scene.append(
        scene.root(),
        NodeKind::Container(Layout::Overlay),
        fixed(30., 30.),
    );
    scene.set_isolated(outer, true);
    scene.set_effects(
        outer,
        Effects {
            opacity: 0.5,
            ..Default::default()
        },
    );
    let inner = scene.append(outer, NodeKind::Container(Layout::Overlay), fixed(20., 20.));
    scene.set_isolated(inner, true);
    scene.set_effects(
        inner,
        Effects {
            opacity: 0.5,
            ..Default::default()
        },
    );
    let red = scene.append(
        inner,
        NodeKind::Rect(Color(255, 0, 0, 255)),
        fixed(20., 20.),
    );
    let d = scene.flush().damage;
    assert_eq!(gpu.render(&scene, &d).unwrap().layer_repaints, 2);
    let quarter = &gpu.readback().unwrap()[..4];
    assert!(near(quarter, &[64, 0, 0, 64]), "{quarter:?}");
    scene.set_transform(inner, Transform { x: 5., y: 5. });
    let d = scene.flush().damage;
    let stats = gpu.render(&scene, &d).unwrap();
    assert_eq!(stats.layer_repaints, 1);
    assert_eq!(stats.layer_cache_hits, 1);
    scene.set_kind(red, NodeKind::Rect(Color(0, 255, 0, 255)));
    let d = scene.flush().damage;
    assert_eq!(gpu.render(&scene, &d).unwrap().layer_repaints, 2);
    let p = gpu.readback().unwrap();
    assert!(near(
        &p[(10 * 64 + 10) * 4..(10 * 64 + 10) * 4 + 4],
        &[0, 64, 0, 64]
    ));
}

#[test]
fn isolated_overflow_and_fractional_translation_reuse_content() {
    let _gpu_fixture = gpu_fixture();
    let mut gpu = GpuRenderer::new(64, 40).unwrap();
    let mut scene = Scene::new(64., 40.);
    let group = scene.append(
        scene.root(),
        NodeKind::Container(Layout::Overlay),
        fixed(10., 10.),
    );
    scene.set_isolated(group, true);
    let child = scene.append(
        group,
        NodeKind::Rect(Color(255, 0, 0, 255)),
        fixed(20., 12.),
    );
    scene.set_transform(child, Transform { x: -5., y: 0. });
    scene.set_transform(group, Transform { x: 15., y: 5. });
    let d = scene.flush().damage;
    gpu.render(&scene, &d).unwrap();
    let p = gpu.readback().unwrap();
    assert_eq!(
        &p[(8 * 64 + 11) * 4..(8 * 64 + 11) * 4 + 4],
        &[255, 0, 0, 255]
    );
    scene.set_transform(group, Transform { x: 15.5, y: 5. });
    let d = scene.flush().damage;
    let stats = gpu.render(&scene, &d).unwrap();
    assert_eq!(stats.layer_repaints, 0);
    let half = gpu.readback().unwrap();
    assert_ne!(p, half);
    scene.set_transform(group, Transform { x: 15.75, y: 5. });
    let d = scene.flush().damage;
    assert_eq!(gpu.render(&scene, &d).unwrap().layer_repaints, 0);
    assert_ne!(half, gpu.readback().unwrap());
    scene.set_isolated(group, false);
    let d = scene.flush().damage;
    gpu.render(&scene, &d).unwrap();
    assert_eq!(gpu.debug_cache_stats().layer_textures, 0);
}

#[test]
fn layer_texture_budget_rasterizes_coarser_and_recovers_after_removal() {
    let _gpu_fixture = gpu_fixture();
    let mut gpu = GpuRenderer::new(8, 8).unwrap();
    let mut scene = Scene::new(8., 8.);
    let huge = scene.append(
        scene.root(),
        NodeKind::Container(Layout::Overlay),
        Style {
            flex_shrink: 0.,
            ..fixed(4096., 4096.)
        },
    );
    scene.set_isolated(huge, true);
    let d = scene.flush().damage;
    gpu.render(&scene, &d).unwrap();
    let issues = gpu.take_issues();
    assert!(
        issues[0].contains("budget of isolated layers (64 MiB)"),
        "{issues:?}"
    );
    let bytes = gpu.debug_cache_stats().layer_bytes;
    assert!(bytes > 0 && bytes <= 64 * 1024 * 1024, "{bytes}");
    scene.remove(huge);
    let d = scene.flush().damage;
    gpu.render(&scene, &d).unwrap();
    assert!(gpu.readback().unwrap().iter().all(|p| *p == 0));
}

#[test]
fn image_count_budget_leaves_images_out_and_recovers_after_removal() {
    let _gpu_fixture = gpu_fixture();
    let mut gpu = GpuRenderer::new(8, 8).unwrap();
    let mut scene = Scene::new(8., 8.);
    let stack = scene.append(
        scene.root(),
        NodeKind::Container(Layout::Overlay),
        fixed(8., 8.),
    );
    let images: Vec<_> = (0..16_385)
        .map(|_| {
            let image = zgx::image::ImageData::new(1, 1, vec![255, 0, 0, 255]).unwrap();
            scene.append(
                stack,
                NodeKind::Image(std::sync::Arc::new(image)),
                fixed(1., 1.),
            )
        })
        .collect();
    let d = scene.flush().damage;
    gpu.render(&scene, &d).unwrap();
    assert!(gpu.take_issues()[0].contains("16,384"));
    scene.remove(images[0]);
    let d = scene.flush().damage;
    gpu.render(&scene, &d).unwrap();
    assert_eq!(gpu.readback().unwrap()[..4], [255, 0, 0, 255]);
}

#[test]
fn blur_inside_isolated_layer_rebuilds_on_backdrop_mutation() {
    let _gpu_fixture = gpu_fixture();
    let mut gpu = GpuRenderer::new(48, 32).unwrap();
    let mut scene = Scene::new(48., 32.);
    let group = scene.append(
        scene.root(),
        NodeKind::Container(Layout::Overlay),
        fixed(48., 32.),
    );
    scene.set_isolated(group, true);
    scene.append(group, NodeKind::Rect(Color(0, 0, 0, 255)), fixed(48., 32.));
    let behind = scene.append(
        group,
        NodeKind::Rect(Color(255, 255, 255, 255)),
        fixed(24., 32.),
    );
    let blur = scene.append(group, NodeKind::Rect(Color(255, 0, 0, 24)), fixed(48., 32.));
    scene.set_effects(
        blur,
        Effects {
            blur_radius: 3.,
            ..Default::default()
        },
    );
    let d = scene.flush().damage;
    gpu.render(&scene, &d).unwrap();
    let before = gpu.readback().unwrap();
    scene.set_kind(behind, NodeKind::Rect(Color(0, 255, 0, 255)));
    let d = scene.flush().damage;
    let stats = gpu.render(&scene, &d).unwrap();
    assert_eq!(stats.layer_repaints, 1);
    assert_eq!(stats.layer_texture_allocations, 0);
    let after = gpu.readback().unwrap();
    assert!(before != after);
    gpu.render(&scene, &[Rect::new(0., 0., 48., 32.)]).unwrap();
    assert!(after == gpu.readback().unwrap());
}

#[test]
fn shared_device_and_fonts_outlive_individual_renderers_with_lazy_atlases() {
    let _gpu_fixture = gpu_fixture();
    use zgx_gpu::GpuContext;
    let context = GpuContext::new().unwrap();
    let fonts = std::rc::Rc::downgrade(&context.text_system());
    let mut first = GpuRenderer::new_with_context(32, 32, &context).unwrap();
    let mut second = GpuRenderer::new_with_context(32, 32, &context).unwrap();
    assert!(first.context().shares_device(&second.context()));
    assert!(std::rc::Rc::ptr_eq(
        &first.text_system(),
        &second.text_system()
    ));
    assert_eq!(first.debug_cache_stats().atlas_bytes, 4);
    assert_eq!(second.debug_cache_stats().atlas_bytes, 4);
    let mut scene = Scene::new(32., 32.);
    scene.append(
        scene.root(),
        NodeKind::Rect(Color(255, 0, 0, 255)),
        fixed(32., 32.),
    );
    let d = scene.flush().damage;
    first.render(&scene, &d).unwrap();
    assert_eq!(first.debug_cache_stats().atlas_bytes, 4);
    assert_eq!(&first.readback().unwrap()[..4], &[255, 0, 0, 255]);
    drop(first);
    drop(context);
    assert!(fonts.upgrade().is_some());
    scene.set_kind(
        scene.root(),
        NodeKind::Text {
            text: "A".into(),
            font_size: 14.,
            color: Color(255, 255, 255, 255),
        },
    );
    let d = scene.flush().damage;
    second.render(&scene, &d).unwrap();
    assert_eq!(second.debug_cache_stats().atlas_bytes, 512 * 512 * 4);
    second.readback().unwrap();
    drop(second);
    assert!(fonts.upgrade().is_none());
}

#[test]
fn glyph_atlas_grows_only_when_a_frame_needs_more_capacity() {
    let _gpu_fixture = gpu_fixture();
    let mut gpu = GpuRenderer::new(800, 800).unwrap();
    let mut scene = Scene::new(800., 800.);
    scene.append(
        scene.root(),
        NodeKind::Text {
            text: "W".into(),
            font_size: 700.,
            color: Color(255, 255, 255, 255),
        },
        fixed(800., 800.),
    );
    let d = scene.flush().damage;
    gpu.render(&scene, &d).unwrap();
    let cache = gpu.debug_cache_stats();
    assert!(cache.atlas_bytes >= 1024 * 1024 * 4);
    assert!(cache.atlas_bytes <= 2048 * 2048 * 4);
    assert!(gpu.readback().unwrap().iter().any(|p| *p > 0));
    let stats = gpu
        .render(&scene, &[Rect::new(0., 0., 800., 800.)])
        .unwrap();
    assert_eq!(stats.glyph_uploads, 0);
    assert_eq!(stats.geometry_rebuilds, 0);
}

#[test]
fn decorated_container_paints_background_before_children() {
    let _gpu_fixture = gpu_fixture();
    let mut gpu = GpuRenderer::new(64, 64).unwrap();
    let mut scene = Scene::new(64., 64.);
    let panel = scene.append(
        scene.root(),
        NodeKind::Panel {
            layout: Layout::Row,
            quad: QuadStyle {
                fill: Color(20, 40, 60, 255),
                radius: 4.,
                border_color: Color(200, 100, 0, 255),
                border_width: 2.,
                ..Default::default()
            },
        },
        Style {
            padding_edges: Some(Insets {
                left: 10.,
                top: 12.,
                right: 3.,
                bottom: 5.,
            }),
            ..fixed(40., 40.)
        },
    );
    scene.append(panel, NodeKind::Rect(Color(255, 0, 0, 255)), fixed(8., 8.));
    let damage = scene.flush().damage;
    gpu.render(&scene, &damage).unwrap();
    let pixels = gpu.readback().unwrap();
    let at = |x: usize, y: usize| &pixels[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4];
    assert_eq!(at(6, 6), &[20, 40, 60, 255]);
    assert_eq!(at(12, 14), &[255, 0, 0, 255]);
    assert_eq!(at(20, 1), &[200, 100, 0, 255]);
}

#[test]
fn composed_fluent_styles_render_and_repaint_without_relayout() {
    let _gpu_fixture = gpu_fixture();
    use zgx::{
        compose::{column, text},
        style::{Styled, Styles, rgb},
        widgets::Ui,
    };
    let mut gpu = GpuRenderer::new(128, 64).unwrap();
    let mut ui = Ui::new(128., 64.);
    let color = ui.signal(rgb(0x204060));
    let read = color.clone();
    let view = ui.mount(
        column()
            .size(128., 64.)
            .p(8.)
            .text_color(rgb(0xffffff))
            .text_size(20.)
            .reactive_style(move || Styles::new().bg(read.get()))
            .child(text("Hello").id("label")),
    );
    let label = view.find("label").unwrap();
    let initial_bounds;
    {
        let mut scene = ui.scene.borrow_mut();
        let damage = scene.flush().damage;
        initial_bounds = scene.bounds(label);
        gpu.render(&scene, &damage).unwrap();
    }
    let first = gpu.readback().unwrap();
    assert_eq!(
        &first[(60 * 128 + 120) * 4..(60 * 128 + 120) * 4 + 4],
        &[32, 64, 96, 255]
    );
    assert!(
        first
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[0] > 220 && p[1] > 220 && p[2] > 220),
        "composed text must render over the panel"
    );
    color.set(rgb(0x603020));
    {
        let mut scene = ui.scene.borrow_mut();
        let report = scene.flush();
        assert_eq!(report.layout_nodes, 0);
        assert_eq!(scene.bounds(label), initial_bounds);
        gpu.render(&scene, &report.damage).unwrap();
    }
    let second = gpu.readback().unwrap();
    assert_eq!(
        &second[(60 * 128 + 120) * 4..(60 * 128 + 120) * 4 + 4],
        &[96, 48, 32, 255]
    );
    gpu.render(&ui.scene.borrow(), &[Rect::new(0., 0., 128., 64.)])
        .unwrap();
    assert_eq!(
        second,
        gpu.readback().unwrap(),
        "damage repaint matches full composition"
    );
}

#[test]
fn fluent_text_padding_and_background_render_as_one_box() {
    let _gpu_fixture = gpu_fixture();
    use zgx::{
        compose::text,
        style::{Styled, rgb},
        widgets::Ui,
    };
    let mut gpu = GpuRenderer::new(96, 48).unwrap();
    let mut ui = Ui::new(96., 48.);
    let mounted = ui.mount(
        text("A")
            .p(8.)
            .bg(rgb(0x206040))
            .text_color(rgb(0xffffff))
            .text_size(20.),
    );
    let mut scene = ui.scene.borrow_mut();
    let report = scene.flush();
    let outer = scene.bounds(mounted.node());
    let children = scene.children(mounted.node());
    assert_eq!(children.len(), 1);
    let inner = scene.bounds(children[0]);
    assert_eq!(inner.x, outer.x + 8.);
    assert_eq!(inner.y, outer.y + 8.);
    assert_eq!(outer.width, inner.width + 16.);
    assert_eq!(outer.height, inner.height + 16.);
    gpu.render(&scene, &report.damage).unwrap();
    let pixels = gpu.readback().unwrap();
    assert_eq!(
        &pixels[(2 * 96 + 2) * 4..(2 * 96 + 2) * 4 + 4],
        &[32, 96, 64, 255]
    );
    assert!(
        pixels
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[0] > 220 && p[1] > 220 && p[2] > 220)
    );
}

#[test]
fn font_selection_changes_pixels_and_measurement_without_stale_cached_glyphs() {
    let _gpu_fixture = gpu_fixture();
    use zgx::text_layout::{FontFamily, FontStyle, TextLayout};
    let mut gpu = GpuRenderer::new(360, 80).unwrap();
    let mut scene = Scene::new(360., 80.);
    let fonts = gpu.text_system();
    let shared = fonts.clone();
    scene.set_font_text_shaper(
        move |text: &str, size, width, font: &FontStyle| -> Box<dyn TextLayout> {
            Box::new(zgx_gpu::text::ShapedText::with_font(
                &mut shared.borrow_mut(),
                text,
                size,
                width,
                font,
            ))
        },
    );
    let id = scene.append(
        scene.root(),
        NodeKind::Text {
            text: "Hamburgefonts ffi".into(),
            color: Color(255, 255, 255, 255),
            font_size: 24.,
        },
        Style::default(),
    );
    let default_damage = scene.flush().damage;
    assert_eq!(gpu.render(&scene, &default_damage).unwrap().shaped_nodes, 1);
    let regular = gpu.readback().unwrap();
    let mut previous = regular.clone();
    for font in [
        FontStyle {
            weight: 700,
            ..Default::default()
        },
        FontStyle {
            italic: true,
            ..Default::default()
        },
        FontStyle {
            family: FontFamily::Monospace,
            ..Default::default()
        },
        FontStyle {
            family: FontFamily::Serif,
            weight: 700,
            italic: true,
            ..Default::default()
        },
    ] {
        scene.set_font(id, font.clone());
        let report = scene.flush();
        assert!(report.layout_nodes > 0);
        let measured = scene.measure_text_with_font("Hamburgefonts ffi", 24., None, &font);
        assert_eq!(scene.bounds(id).width, measured.0);
        assert_eq!(gpu.render(&scene, &report.damage).unwrap().shaped_nodes, 1);
        let pixels = gpu.readback().unwrap();
        assert_ne!(
            pixels, previous,
            "font change must affect rasterized pixels"
        );
        assert_eq!(
            gpu.render(&scene, &[Rect::new(0., 0., 360., 80.)])
                .unwrap()
                .shaped_nodes,
            0
        );
        assert_eq!(gpu.readback().unwrap(), pixels);
        scene.set_font(id, font);
        assert!(scene.flush().is_idle());
        previous = pixels;
    }
    scene.set_font(id, FontStyle::default());
    let damage = scene.flush().damage;
    gpu.render(&scene, &damage).unwrap();
    assert_eq!(gpu.readback().unwrap(), regular);
}

#[test]
fn declarative_editors_keep_styled_text_caret_and_selection_correct_when_resized() {
    let _gpu_fixture = gpu_fixture();
    use zgx::{
        compose::{column, text_area, text_input},
        input::{InputEvent, Key, Modifiers},
        style::{Styled, Styles, rgb},
        text_layout::{FontFamily, FontStyle, TextLayout},
        widgets::Ui,
    };
    let mut gpu = GpuRenderer::new(320, 200).unwrap();
    let mut ui = Ui::new(320., 200.);
    let fonts = gpu.text_system();
    ui.scene.borrow_mut().set_font_text_shaper(
        move |text: &str, size, width, font: &FontStyle| -> Box<dyn TextLayout> {
            Box::new(zgx_gpu::text::ShapedText::with_font(
                &mut fonts.borrow_mut(),
                text,
                size,
                width,
                font,
            ))
        },
    );
    let value = ui.signal("MMMM WWWW iii 0123".to_owned());
    let multiline = ui.signal("First line\nSecond line\nThird line".to_owned());
    let weight = ui.signal(700);
    let inherited_weight = weight.clone();
    let width = ui.signal(280.);
    let height = ui.signal(108.);
    let input_width = width.clone();
    let area_width = width.clone();
    let area_height = height.clone();
    let mounted = ui.mount(
        column()
            .size(320., 200.)
            .gap(8.)
            .bg(rgb(0x101820))
            .text_color(rgb(0xff4020))
            .text_size(22.)
            .font_family(FontFamily::Monospace)
            .reactive_style(move || Styles::new().font_weight(inherited_weight.get()))
            .child(
                text_input("Name", value.clone())
                    .id("input")
                    .h(44.)
                    .p(6.)
                    .bg(rgb(0x203040))
                    .rounded(0.)
                    .reactive_style(move || Styles::new().w(input_width.get())),
            )
            .child(
                text_area("Details", multiline.clone())
                    .id("area")
                    .p(6.)
                    .bg(rgb(0x203040))
                    .rounded(0.)
                    .reactive_style(move || Styles::new().w(area_width.get()).h(area_height.get())),
            ),
    );
    let render = |ui: &Ui, gpu: &mut GpuRenderer| {
        ui.prepare_frame();
        let mut scene = ui.scene.borrow_mut();
        let damage = scene.flush().damage;
        gpu.render(&scene, &damage).unwrap();
        let partial = gpu.readback().unwrap();
        gpu.render(&scene, &[Rect::new(0., 0., 320., 200.)])
            .unwrap();
        assert!(
            partial == gpu.readback().unwrap(),
            "incremental editor damage must reconstruct the full frame"
        );
        partial
    };
    let initial = render(&ui, &mut gpu);
    assert!(
        initial
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[0] > 200 && p[1] < 100 && p[2] < 80)
            .count()
            > 100,
        "inherited foreground must color actual glyph pixels"
    );
    weight.set(400);
    let regular = render(&ui, &mut gpu);
    assert!(
        regular != initial,
        "changing inherited font weight must change actual editor glyphs"
    );
    weight.set(700);
    assert!(
        render(&ui, &mut gpu) == initial,
        "restoring inherited typography restores both editors"
    );
    for id in ["input", "area"] {
        ui.input.focus(&ui.scene, Some(mounted.find(id).unwrap()));
        ui.dispatch(InputEvent::KeyDown {
            key: Key::End,
            modifiers: Modifiers::default(),
            repeat: false,
        });
        let pixels = render(&ui, &mut gpu);
        let editor = ui.focused_editor().unwrap();
        let caret = ui.scene.borrow().bounds(editor.caret);
        let x = caret.x.round() as usize;
        let y = (caret.y + caret.height / 2.).floor() as usize;
        let at = (y * 320 + x) * 4;
        assert!(
            pixels[at] > 200 && pixels[at + 1] < 100,
            "caret must inherit the red foreground: {:?}",
            &pixels[at..at + 4]
        );
        for _ in 0..4 {
            ui.dispatch(InputEvent::KeyDown {
                key: Key::ArrowLeft,
                modifiers: Modifiers {
                    shift: true,
                    ..Default::default()
                },
                repeat: false,
            });
        }
        let selected = render(&ui, &mut gpu);
        assert_ne!(
            selected, pixels,
            "keyboard selection must change rendered pixels"
        );
        assert!(!editor.copy().is_empty());
        assert!(
            selected
                .as_chunks::<4>()
                .0
                .iter()
                .any(|p| p[0].abs_diff(48) <= 1
                    && p[1].abs_diff(81) <= 1
                    && p[2].abs_diff(127) <= 1),
            "selection background must alpha-composite over the styled editor surface"
        );
        // Only allocation changes: neither text nor selection is reassigned.
        for (w, h) in [(110., 48.), (280., 108.), (150., 64.), (280., 108.)] {
            width.set(w);
            height.set(h);
            let resized = render(&ui, &mut gpu);
            if w == 280. {
                assert!(
                    resized == selected,
                    "growing the {id} editor restores its exact selection and text pixels"
                );
            }
        }
        ui.dispatch(InputEvent::Text("X".into()));
        assert_ne!(
            render(&ui, &mut gpu),
            selected,
            "editing selected text must repaint"
        );
    }
}

#[test]
fn declarative_virtual_rows_clip_padding_and_repair_scroll_damage() {
    let _gpu_fixture = gpu_fixture();
    use zgx::{compose::prelude::*, widgets::Ui};
    let mut gpu = GpuRenderer::new(128, 96).unwrap();
    let mut ui = Ui::new(128., 96.);
    let offset = ui.signal(0.);
    let height = ui.signal(80.);
    let read_height = height.clone();
    ui.mount(
        virtual_list(
            offset.clone(),
            20.,
            2,
            || 100_000,
            |i| i,
            |_, key, _| {
                div().size(112., 20.).bg(if key.is_multiple_of(2) {
                    rgb(0xee3322)
                } else {
                    rgb(0x2244ee)
                })
            },
        )
        .w(128.)
        .p(8.)
        .bg(rgb(0x102030))
        .reactive_style(move || Styles::new().h(read_height.get())),
    );
    for (scroll, viewport_height) in [
        (0., 80.),
        (13., 80.),
        (56013., 80.),
        (56013., 60.),
        (1999900., 80.),
        (0., 80.),
    ] {
        height.set(viewport_height);
        offset.set(scroll);
        ui.prepare_frame();
        let damage = ui.scene.borrow_mut().flush().damage;
        gpu.render(&ui.scene.borrow(), &damage).unwrap();
        let partial = gpu.readback().unwrap();
        gpu.render(&ui.scene.borrow(), &[Rect::new(0., 0., 128., 96.)])
            .unwrap();
        assert_eq!(
            partial,
            gpu.readback().unwrap(),
            "scroll {scroll} height {viewport_height}"
        );
        let pixel = |x: usize, y: usize| &partial[(y * 128 + x) * 4..(y * 128 + x) * 4 + 4];
        assert_eq!(
            pixel(64, 4),
            &[16, 32, 48, 255],
            "rows must not paint top padding"
        );
        assert_eq!(
            pixel(64, viewport_height as usize - 4),
            &[16, 32, 48, 255],
            "rows must not paint bottom padding"
        );
        let key = ((offset.get() + 2.) / 20.).floor() as usize;
        assert_eq!(
            pixel(64, 10),
            if key.is_multiple_of(2) {
                &[238, 51, 34, 255]
            } else {
                &[34, 68, 238, 255]
            }
        );
        assert!(ui.scene.borrow().paint_items().count() < 100);
    }
}

#[test]
fn declarative_slider_damage_clears_old_thumb_and_resized_surface() {
    let _gpu_fixture = gpu_fixture();
    use zgx::{compose::prelude::*, widgets::Ui};
    let mut gpu = GpuRenderer::new(420, 80).unwrap();
    let mut ui = Ui::new(420., 80.);
    let value = ui.signal(0.);
    let width = ui.signal(320.);
    let color = ui.signal(rgb(0x22dd77));
    let read_width = width.clone();
    let read_color = color.clone();
    let mounted = ui.mount(
        slider("Level", value.clone(), 0.0..=100.)
            .h(60.)
            .px(16.)
            .py(10.)
            .bg(rgb(0x102030))
            .reactive_style(move || {
                Styles::new()
                    .w(read_width.get())
                    .text_color(read_color.get())
            }),
    );
    for (amount, allocation, tint) in [
        (0., 320., rgb(0x22dd77)),
        (75., 320., rgb(0x22dd77)),
        (75., 120., rgb(0xcc4466)),
        (100., 400., rgb(0xcc4466)),
        (50., 320., rgb(0x22dd77)),
    ] {
        value.set(amount);
        width.set(allocation);
        color.set(tint);
        ui.prepare_frame();
        let damage = ui.scene.borrow_mut().flush().damage;
        gpu.render(&ui.scene.borrow(), &damage).unwrap();
        let partial = gpu.readback().unwrap();
        gpu.render(&ui.scene.borrow(), &[Rect::new(0., 0., 420., 80.)])
            .unwrap();
        assert_eq!(
            partial,
            gpu.readback().unwrap(),
            "value={amount}, width={allocation}"
        );
        let scene = ui.scene.borrow();
        let thumb = scene.bounds(scene.children(mounted.node())[2]);
        let x = (thumb.x + thumb.width / 2.) as usize;
        let y = (thumb.y + thumb.height / 2.) as usize;
        assert_eq!(
            &partial[(y * 420 + x) * 4..(y * 420 + x) * 4 + 4],
            &[tint.0, tint.1, tint.2, tint.3]
        );
        assert!(thumb.x >= 16. && thumb.x + thumb.width <= allocation - 16.);
    }
}

#[test]
fn declarative_progress_composites_fill_without_layout_or_padding_bleed() {
    let _gpu_fixture = gpu_fixture();
    use zgx::{compose::prelude::*, widgets::Ui};
    let mut gpu = GpuRenderer::new(420, 80).unwrap();
    let mut ui = Ui::new(420., 80.);
    let value = ui.signal(0.);
    let width = ui.signal(208.);
    let read_width = width.clone();
    ui.mount(
        progress("Download", value.clone())
            .h(24.)
            .p(4.)
            .bg(rgb(0x102030))
            .text_color(rgb(0x22dd77))
            .reactive_style(move || Styles::new().w(read_width.get())),
    );
    let mut previous_width = 0.;
    for (amount, allocation) in [
        (0., 208.),
        (0.25, 208.),
        (0.75, 208.),
        (1., 208.),
        (0.5, 408.),
        (0., 408.),
        (1., 108.),
    ] {
        value.set(amount);
        width.set(allocation);
        ui.prepare_frame();
        let frame = ui.scene.borrow_mut().flush();
        if allocation == previous_width {
            assert_eq!(frame.layout_nodes, 0);
        }
        previous_width = allocation;
        gpu.render(&ui.scene.borrow(), &frame.damage).unwrap();
        let partial = gpu.readback().unwrap();
        gpu.render(&ui.scene.borrow(), &[Rect::new(0., 0., 420., 80.)])
            .unwrap();
        assert_eq!(
            partial,
            gpu.readback().unwrap(),
            "progress={amount} width={allocation}"
        );
        let filled = partial
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| **p == [34, 221, 119, 255])
            .count();
        assert_eq!(filled, ((allocation - 8.) * amount) as usize * 16);
        for (x, y) in [(2, 12), (4, 2), (4, 22), (allocation as usize - 2, 12)] {
            assert_eq!(
                &partial[(y * 420 + x) * 4..(y * 420 + x) * 4 + 4],
                &[16, 32, 48, 255],
                "padding must stay track-colored"
            );
        }
    }
}

#[test]
fn declarative_images_repaint_source_and_allocated_content_without_padding_bleed() {
    let _gpu_fixture = gpu_fixture();
    use std::sync::Arc;
    use zgx::{compose::prelude::*, image::ImageData, widgets::Ui};
    let bitmap = |width, color: [u8; 4]| {
        Arc::new(ImageData::new(width, 4, color.repeat(width as usize * 4)).unwrap())
    };
    let red = bitmap(4, [220, 30, 40, 255]);
    let blue = bitmap(4, [30, 80, 220, 255]);
    let green = bitmap(8, [30, 220, 80, 255]);
    let mut gpu = GpuRenderer::new(160, 80).unwrap();
    let mut ui = Ui::new(160., 80.);
    let source = ui.signal(red.clone());
    let width = ui.signal(100.);
    let read_source = source.clone();
    let read_width = width.clone();
    ui.mount(
        image_signal("Preview", move || read_source.get())
            .h(60.)
            .p(4.)
            .bg(rgb(0x102030))
            .reactive_style(move || Styles::new().w(read_width.get())),
    );
    for (index, (bitmap, allocation, expected)) in [
        (red.clone(), 100., [220, 30, 40, 255]),
        (blue, 100., [30, 80, 220, 255]),
        (green, 140., [30, 220, 80, 255]),
        (red, 60., [220, 30, 40, 255]),
    ]
    .into_iter()
    .enumerate()
    {
        source.set(bitmap);
        width.set(allocation);
        ui.prepare_frame();
        let frame = ui.scene.borrow_mut().flush();
        if index == 1 {
            assert_eq!(frame.layout_nodes, 0);
        }
        gpu.render(&ui.scene.borrow(), &frame.damage).unwrap();
        let partial = gpu.readback().unwrap();
        gpu.render(&ui.scene.borrow(), &[Rect::new(0., 0., 160., 80.)])
            .unwrap();
        assert_eq!(
            partial,
            gpu.readback().unwrap(),
            "source transition {index}"
        );
        assert_eq!(
            partial
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|p| **p == expected)
                .count(),
            (allocation as usize - 8) * 52
        );
        for (x, y) in [(2, 30), (10, 2), (10, 58), (allocation as usize - 2, 30)] {
            assert_eq!(
                &partial[(y * 160 + x) * 4..(y * 160 + x) * 4 + 4],
                &[16, 32, 48, 255]
            );
        }
    }
}

#[test]
fn declarative_scroll_clips_children_and_repairs_damage_without_relayout() {
    let _gpu_fixture = gpu_fixture();
    use zgx::{compose::prelude::*, widgets::Ui};
    let mut gpu = GpuRenderer::new(128, 96).unwrap();
    let mut ui = Ui::new(128., 96.);
    let offset = ui.signal(0.);
    let height = ui.signal(80.);
    let read_height = height.clone();
    ui.mount(
        scroll(offset.clone())
            .w(128.)
            .p(8.)
            .bg(rgb(0x102030))
            .reactive_style(move || Styles::new().h(read_height.get()))
            .children((0..8).map(|i| {
                div().size(112., 20.).shrink_0().bg(if i % 2 == 0 {
                    rgb(0xee3322)
                } else {
                    rgb(0x2244ee)
                })
            })),
    );
    let mut previous_height = 0.;
    for (position, allocation) in [
        (0., 80.),
        (13., 80.),
        (55., 80.),
        (55., 60.),
        (1000., 80.),
        (0., 80.),
    ] {
        height.set(allocation);
        offset.set(position);
        ui.prepare_frame();
        let frame = ui.scene.borrow_mut().flush();
        if allocation == previous_height {
            assert_eq!(frame.layout_nodes, 0);
        }
        previous_height = allocation;
        gpu.render(&ui.scene.borrow(), &frame.damage).unwrap();
        let partial = gpu.readback().unwrap();
        gpu.render(&ui.scene.borrow(), &[Rect::new(0., 0., 128., 96.)])
            .unwrap();
        assert_eq!(
            partial,
            gpu.readback().unwrap(),
            "scroll={position}, height={allocation}"
        );
        let pixel = |x: usize, y: usize| &partial[(y * 128 + x) * 4..(y * 128 + x) * 4 + 4];
        assert_eq!(pixel(64, 4), &[16, 32, 48, 255]);
        assert_eq!(pixel(64, allocation as usize - 4), &[16, 32, 48, 255]);
        let row = ((offset.get() + 2.) / 20.).floor() as usize;
        assert_eq!(
            pixel(64, 10),
            if row.is_multiple_of(2) {
                &[238, 51, 34, 255]
            } else {
                &[34, 68, 238, 255]
            }
        );
        assert!(offset.get() <= (160. - (allocation - 16.)).max(0.));
    }
}

#[test]
fn focused_scroll_descendant_is_revealed_with_correct_damage() {
    let _gpu_fixture = gpu_fixture();
    use zgx::{compose::prelude::*, widgets::Ui};
    let mut gpu = GpuRenderer::new(128, 96).unwrap();
    let mut ui = Ui::new(128., 96.);
    let offset = ui.signal(0.);
    let mounted = ui.mount(
        scroll(offset.clone())
            .size(128., 80.)
            .p(8.)
            .bg(rgb(0x102030))
            .child(
                button()
                    .id("first")
                    .size(80., 32.)
                    .bg(rgb(0xee3322))
                    .focus(|s| s),
            )
            .child(div().h(100.))
            .child(
                button()
                    .id("last")
                    .size(80., 32.)
                    .bg(rgb(0x2244ee))
                    .focus(|s| s),
            ),
    );
    ui.prepare_frame();
    let damage = ui.scene.borrow_mut().flush().damage;
    gpu.render(&ui.scene.borrow(), &damage).unwrap();
    for (id, expected_offset, expected_pixel) in [
        ("last", 100., [34, 68, 238, 255]),
        ("first", 0., [238, 51, 34, 255]),
    ] {
        let target = mounted.find(id).unwrap();
        assert!(ui.input.focus(&ui.scene, Some(target)));
        ui.prepare_frame();
        assert_eq!(offset.get(), expected_offset);
        let bounds = ui.scene.borrow().bounds(target);
        assert!(bounds.y >= 8. && bounds.y + bounds.height <= 72.);
        let frame = ui.scene.borrow_mut().flush();
        assert_eq!(frame.layout_nodes, 0);
        gpu.render(&ui.scene.borrow(), &frame.damage).unwrap();
        let partial = gpu.readback().unwrap();
        gpu.render(&ui.scene.borrow(), &[Rect::new(0., 0., 128., 96.)])
            .unwrap();
        assert_eq!(partial, gpu.readback().unwrap());
        let y = (bounds.y + 10.) as usize;
        assert_eq!(
            &partial[(y * 128 + 20) * 4..(y * 128 + 20) * 4 + 4],
            &expected_pixel
        );
    }
}

#[test]
fn horizontal_scroll_clips_padding_and_repairs_compositor_damage() {
    let _gpu_fixture = gpu_fixture();
    use zgx::{compose::prelude::*, widgets::Ui};
    let mut gpu = GpuRenderer::new(176, 96).unwrap();
    let mut ui = Ui::new(176., 96.);
    let offset = ui.signal(0.);
    let width = ui.signal(128.);
    let read_width = width.clone();
    ui.mount(
        scroll_x(offset.clone())
            .h(80.)
            .p(8.)
            .bg(rgb(0x102030))
            .reactive_style(move || Styles::new().w(read_width.get()))
            .children((0..8).map(|i| {
                div().size(40., 64.).shrink_0().bg(if i % 2 == 0 {
                    rgb(0xee3322)
                } else {
                    rgb(0x2244ee)
                })
            })),
    );
    let mut previous_width = 0.;
    for (position, allocation) in [
        (0., 128.),
        (13., 128.),
        (55., 128.),
        (1000., 128.),
        (1000., 160.),
        (0., 128.),
    ] {
        width.set(allocation);
        offset.set(position);
        ui.prepare_frame();
        let frame = ui.scene.borrow_mut().flush();
        if allocation == previous_width {
            assert_eq!(frame.layout_nodes, 0);
        }
        previous_width = allocation;
        gpu.render(&ui.scene.borrow(), &frame.damage).unwrap();
        let partial = gpu.readback().unwrap();
        gpu.render(&ui.scene.borrow(), &[Rect::new(0., 0., 176., 96.)])
            .unwrap();
        assert_eq!(
            partial,
            gpu.readback().unwrap(),
            "offset={position} width={allocation}"
        );
        let pixel = |x: usize, y: usize| &partial[(y * 176 + x) * 4..(y * 176 + x) * 4 + 4];
        assert_eq!(pixel(4, 40), &[16, 32, 48, 255]);
        assert_eq!(pixel(allocation as usize - 4, 40), &[16, 32, 48, 255]);
        assert_eq!(pixel(12, 4), &[16, 32, 48, 255]);
        let cell = ((offset.get() + 2.) / 40.).floor() as usize;
        assert_eq!(
            pixel(10, 40),
            if cell.is_multiple_of(2) {
                &[238, 51, 34, 255]
            } else {
                &[34, 68, 238, 255]
            }
        );
        assert!(offset.get() <= 320. - (allocation - 16.));
    }
}

#[test]
fn overlay_scrollbar_thumb_translates_and_clears_when_content_shrinks() {
    let _gpu_fixture = gpu_fixture();
    use zgx::{compose::prelude::*, widgets::Ui};
    let mut gpu = GpuRenderer::new(128, 96).unwrap();
    let mut ui = Ui::new(128., 96.);
    let offset = ui.signal(0.);
    let extent = ui.signal(400.);
    let read_extent = extent.clone();
    ui.mount(
        scroll(offset.clone())
            .size(128., 80.)
            .p(8.)
            .bg(rgb(0x102030))
            .scrollbar(true)
            .child(
                div()
                    .bg(rgb(0x442211))
                    .reactive_style(move || Styles::new().w(112.).h(read_extent.get())),
            ),
    );
    let mut previous_extent = 0.;
    for (position, height, thumb_y) in [
        (0., 400., Some(8)),
        (168., 400., Some(28)),
        (336., 400., Some(48)),
        (0., 40., None),
        (0., 400., Some(8)),
    ] {
        extent.set(height);
        offset.set(position);
        ui.prepare_frame();
        let frame = ui.scene.borrow_mut().flush();
        if height == previous_extent {
            assert_eq!(frame.layout_nodes, 0);
        }
        previous_extent = height;
        gpu.render(&ui.scene.borrow(), &frame.damage).unwrap();
        let partial = gpu.readback().unwrap();
        gpu.render(&ui.scene.borrow(), &[Rect::new(0., 0., 128., 96.)])
            .unwrap();
        assert_eq!(partial, gpu.readback().unwrap());
        let accent = [94, 165, 255, 255];
        let visible: Vec<_> = partial
            .as_chunks::<4>()
            .0
            .iter()
            .enumerate()
            .filter(|(_, p)| **p == accent)
            .map(|(i, _)| (i % 128, i / 128))
            .collect();
        match thumb_y {
            // A rounded thumb 6 px across the 8 px track, its ends 2 px in
            // from the 24 px its position spans.
            Some(y) => {
                assert!(
                    visible.contains(&(116, y + 12)),
                    "thumb at {y}: {visible:?}"
                );
                assert!(
                    visible
                        .iter()
                        .all(|(x, row)| *x >= 113 && *x < 119 && *row >= y + 2 && *row < y + 22)
                );
            }
            None => assert!(visible.is_empty()),
        }
    }
}

#[test]
fn virtual_keyboard_end_home_repaint_only_retained_visible_rows() {
    let _gpu_fixture = gpu_fixture();
    use zgx::{
        compose::prelude::*,
        input::{InputEvent, Key, Modifiers},
        widgets::Ui,
    };
    let mut gpu = GpuRenderer::new(128, 96).unwrap();
    let mut ui = Ui::new(128., 96.);
    let mounted = ui.mount(
        virtual_list(
            ui.signal(0.),
            20.,
            1,
            || 1_000_000,
            |i| i,
            |_, i, _| {
                div().size(112., 20.).bg(if i.is_multiple_of(2) {
                    rgb(0xee3322)
                } else {
                    rgb(0x2244ee)
                })
            },
        )
        .size(128., 96.)
        .p(8.)
        .bg(rgb(0x102030))
        .keyboard_navigation(true),
    );
    ui.prepare_frame();
    let damage = ui.scene.borrow_mut().flush().damage;
    gpu.render(&ui.scene.borrow(), &damage).unwrap();
    assert!(ui.input.focus(&ui.scene, Some(mounted.node())));
    for (key, position, color) in [
        (Key::End, 1_000_000, [34, 68, 238, 255]),
        (Key::Home, 1, [238, 51, 34, 255]),
    ] {
        ui.dispatch(InputEvent::KeyDown {
            key,
            modifiers: Modifiers::default(),
            repeat: false,
        });
        ui.prepare_frame();
        let focused = ui.input.focused().unwrap();
        assert_eq!(
            ui.semantics.borrow().get(focused).unwrap().position_in_set,
            Some(position)
        );
        let bounds = ui.scene.borrow().bounds(focused);
        assert!(bounds.y >= 8. && bounds.y + bounds.height <= 88.);
        let damage = ui.scene.borrow_mut().flush().damage;
        gpu.render(&ui.scene.borrow(), &damage).unwrap();
        let partial = gpu.readback().unwrap();
        gpu.render(&ui.scene.borrow(), &[Rect::new(0., 0., 128., 96.)])
            .unwrap();
        assert_eq!(partial, gpu.readback().unwrap());
        let y = (bounds.y + 10.) as usize;
        assert_eq!(&partial[(y * 128 + 64) * 4..(y * 128 + 64) * 4 + 4], &color);
        assert!(ui.scene.borrow().paint_items().count() < 100);
    }
}

#[test]
fn independent_devices_render_concurrently_with_bounded_workers() {
    let _gpu_fixture = gpu_fixture();
    exercise_independent_devices(4, 1);
}

/// Explicit diagnostic load, excluded from ordinary CI and workspace runs.
#[test]
#[ignore = "opt-in independent-device lifecycle stress; may consume substantial driver resources"]
fn independent_device_lifecycle_stress() {
    let _gpu_fixture = gpu_fixture();
    let setting = |name: &str, default: usize, limit: usize| {
        let value = std::env::var(name)
            .map(|v| v.parse::<usize>().expect("positive integer setting"))
            .unwrap_or(default);
        assert!(
            (1..=limit).contains(&value),
            "{name} must be between 1 and {limit}"
        );
        value
    };
    let workers = setting("ZGX_GPU_STRESS_WORKERS", 24, 64);
    let iterations = setting("ZGX_GPU_STRESS_ITERATIONS", 10, 100);
    eprintln!("independent device stress: {workers} workers, {iterations} iterations each");
    exercise_independent_devices(workers, iterations);
}

fn exercise_independent_devices(worker_count: usize, iterations: usize) {
    let start = std::sync::Arc::new(std::sync::Barrier::new(worker_count));
    let workers: Vec<_> = (0..worker_count)
        .map(|index| {
            let start = start.clone();
            std::thread::spawn(move || {
                start.wait();
                for _ in 0..iterations {
                    let mut gpu = GpuRenderer::new(32, 32).unwrap();
                    let mut scene = Scene::new(32., 32.);
                    let color = Color(40 + (index % 4) as u8 * 40, 60, 80, 255);
                    scene.append(scene.root(), NodeKind::Rect(color), fixed(32., 32.));
                    let damage = scene.flush().damage;
                    gpu.render(&scene, &damage).unwrap();
                    let pixels = gpu.readback().unwrap();
                    assert!(
                        pixels
                            .as_chunks::<4>()
                            .0
                            .iter()
                            .all(|pixel| *pixel == [color.0, color.1, color.2, color.3])
                    );
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
}

#[test]
fn declarative_portals_restore_pixels_and_track_clipped_anchors_without_layout() {
    let _gpu_fixture = gpu_fixture();
    use zgx::{compose::prelude::*, widgets::Ui};
    let mut gpu = GpuRenderer::new(128, 96).unwrap();
    let mut ui = Ui::new(128., 96.);
    let open = ui.signal(false);
    let mounted = ui.mount(
        column().size(128., 96.).bg(rgb(0x285078)).child(
            modal("Dialog", open.clone())
                .size(64., 40.)
                .p(0.)
                .rounded(0.)
                .bg(rgb(0xee3322)),
        ),
    );
    let mut render = |ui: &Ui| {
        ui.prepare_frame();
        let report = ui.scene.borrow_mut().flush();
        gpu.render(&ui.scene.borrow(), &report.damage).unwrap();
        let partial = gpu.readback().unwrap();
        gpu.render(&ui.scene.borrow(), &[Rect::new(0., 0., 128., 96.)])
            .unwrap();
        assert_eq!(partial, gpu.readback().unwrap());
        (partial, report.layout_nodes)
    };
    let (closed, _) = render(&ui);
    assert!(
        closed
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| *pixel == [40, 80, 120, 255])
    );
    open.set(true);
    let (shown, _) = render(&ui);
    let pixel = |pixels: &[u8], x: usize, y: usize| {
        pixels[(y * 128 + x) * 4..(y * 128 + x) * 4 + 4].to_vec()
    };
    assert_eq!(pixel(&shown, 64, 48), [238, 51, 34, 255]);
    assert!(pixel(&shown, 4, 4)[0] < 40); // modal backdrop dims the page
    open.set(false);
    assert_eq!(render(&ui).0, closed);
    mounted.unmount();

    let open = ui.signal(true);
    let movement = ui.signal((0., 0.));
    let read = movement.clone();
    let mounted = ui.mount(
        column().size(128., 96.).bg(rgb(0x285078)).child(
            column().size(128., 32.).p(8.).overflow_hidden().child(
                popover(
                    "Details",
                    open.clone(),
                    div().size(16., 16.).reactive_style(move || {
                        let (x, y) = read.get();
                        Styles::new().translate(x, y)
                    }),
                )
                .size(48., 24.)
                .p(0.)
                .rounded(0.)
                .bg(rgb(0xee3322)),
            ),
        ),
    );
    let (initial, _) = render(&ui);
    assert_eq!(pixel(&initial, 16, 44), [238, 51, 34, 255]); // escapes anchor clip
    movement.set((40., 16.));
    let (moved, layout_nodes) = render(&ui);
    assert_eq!(layout_nodes, 0);
    assert_eq!(pixel(&moved, 16, 44), [40, 80, 120, 255]);
    assert_eq!(pixel(&moved, 64, 60), [238, 51, 34, 255]);
    open.set(false);
    assert_eq!(render(&ui).0, closed);
    mounted.unmount();
}

#[test]
fn declarative_menu_focus_and_dismissal_match_full_repaint() {
    let _gpu_fixture = gpu_fixture();
    use zgx::{
        compose::prelude::*,
        input::{InputEvent, Key, Modifiers},
        widgets::Ui,
    };
    let mut gpu = GpuRenderer::new(128, 128).unwrap();
    let mut ui = Ui::new(128., 128.);
    let open = ui.signal(false);
    let item = |label: &str| {
        menu_item(label)
            .size(88., 20.)
            .p(0.)
            .rounded(0.)
            .bg(rgb(0x224466))
            .focus(|s| s.bg(rgb(0xeeaa22)))
    };
    let view = ui.mount(
        column().size(128., 128.).p(8.).bg(rgb(0x142030)).child(
            menu(
                "Actions",
                open.clone(),
                button()
                    .id("anchor")
                    .size(32., 20.)
                    .p(0.)
                    .child(text("Menu")),
            )
            .size(96., 68.)
            .p(4.)
            .rounded(0.)
            .bg(rgb(0x303840))
            .child(item("One").id("one"))
            .child(item("Two").disabled(true))
            .child(item("Three").id("three")),
        ),
    );
    let mut render = |ui: &Ui| {
        ui.prepare_frame();
        let report = ui.scene.borrow_mut().flush();
        gpu.render(&ui.scene.borrow(), &report.damage).unwrap();
        let partial = gpu.readback().unwrap();
        gpu.render(&ui.scene.borrow(), &[Rect::new(0., 0., 128., 128.)])
            .unwrap();
        assert_eq!(partial, gpu.readback().unwrap());
        (partial, report.layout_nodes)
    };
    ui.prepare_frame();
    ui.input.focus(&ui.scene, view.find("anchor"));
    let closed = render(&ui).0;
    open.set(true);
    let (first, _) = render(&ui);
    let focused = view.find("one").unwrap();
    assert_eq!(ui.input.focused(), Some(focused));
    let sample = |ui: &Ui, pixels: &[u8], node| {
        let bounds = ui.scene.borrow().bounds(node);
        let index = ((bounds.y as usize + 10) * 128 + bounds.x as usize + 80) * 4;
        pixels[index..index + 4].to_vec()
    };
    assert_eq!(sample(&ui, &first, focused), [238, 170, 34, 255]);
    ui.dispatch(InputEvent::KeyDown {
        key: Key::ArrowDown,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    let (next, layout_nodes) = render(&ui);
    assert_eq!(layout_nodes, 0);
    let focused = view.find("three").unwrap();
    assert_eq!(ui.input.focused(), Some(focused));
    assert_eq!(sample(&ui, &next, focused), [238, 170, 34, 255]);
    ui.dispatch(InputEvent::KeyDown {
        key: Key::Escape,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    assert!(!open.get());
    assert!(
        render(&ui).0 == closed,
        "dismissal must restore the focused-trigger baseline"
    );
}

#[test]
fn overflowing_menu_keeps_keyboard_focus_pixels_inside_padded_viewport() {
    let _gpu_fixture = gpu_fixture();
    use zgx::{
        compose::prelude::*,
        input::{InputEvent, Key, Modifiers},
        widgets::Ui,
    };
    let mut gpu = GpuRenderer::new(128, 96).unwrap();
    let mut ui = Ui::new(128., 96.);
    let view = ui.mount(
        column().size(128., 96.).bg(rgb(0x142030)).child(
            menu(
                "Long",
                ui.signal(true),
                button().size(32., 20.).p(0.).child("Menu"),
            )
            .id("panel")
            .w(96.)
            .max_h(60.)
            .p(4.)
            .rounded(0.)
            .bg(rgb(0x303840))
            .children((0..20).map(|i| {
                menu_item(format!("Row {i}"))
                    .id(format!("item{i}"))
                    .h(16.)
                    .p(0.)
                    .text_size(8.)
                    .rounded(0.)
                    .bg(rgb(0x224466))
                    .focus(|s| s.bg(rgb(0xeeaa22)))
            })),
        ),
    );
    ui.prepare_frame();
    let damage = ui.scene.borrow_mut().flush().damage;
    gpu.render(&ui.scene.borrow(), &damage).unwrap();
    for (key, id) in [(Key::End, "item19"), (Key::Home, "item0")] {
        ui.dispatch(InputEvent::KeyDown {
            key,
            modifiers: Modifiers::default(),
            repeat: false,
        });
        ui.prepare_frame();
        let report = ui.scene.borrow_mut().flush();
        assert_eq!(report.layout_nodes, 0);
        gpu.render(&ui.scene.borrow(), &report.damage).unwrap();
        let partial = gpu.readback().unwrap();
        gpu.render(&ui.scene.borrow(), &[Rect::new(0., 0., 128., 96.)])
            .unwrap();
        assert!(
            partial == gpu.readback().unwrap(),
            "menu scrolling damage must match a full repaint"
        );
        let focused = view.find(id).unwrap();
        assert_eq!(ui.input.focused(), Some(focused));
        let bounds = ui.scene.borrow().bounds(focused);
        let index = ((bounds.y as usize + 8) * 128 + bounds.x as usize + 80) * 4;
        assert_eq!(&partial[index..index + 4], &[238, 170, 34, 255]);
        let panel = ui.scene.borrow().bounds(view.find("panel").unwrap());
        for y in [panel.y as usize + 2, (panel.y + panel.height) as usize - 2] {
            let index = (y * 128 + panel.x as usize + 80) * 4;
            assert_eq!(&partial[index..index + 4], &[48, 56, 64, 255]);
        }
    }
}

#[test]
fn submenu_side_flip_and_dismissal_match_full_repaint_without_layout() {
    let _gpu_fixture = gpu_fixture();
    use zgx::{
        compose::prelude::*,
        input::{InputEvent, Key, Modifiers},
        widgets::Ui,
    };
    let mut gpu = GpuRenderer::new(400, 240).unwrap();
    let mut ui = Ui::new(400., 240.);
    let open = ui.signal(false);
    let child_open = ui.signal(false);
    let movement = ui.signal(0.);
    let read = movement.clone();
    let view = ui.mount(
        column()
            .p(20.)
            .reactive_style(move || Styles::new().translate(read.get(), 0.))
            .child(
                menu(
                    "Actions",
                    open.clone(),
                    button().size(60., 20.).p(0.).child("Actions"),
                )
                .id("parent")
                .w(160.)
                .p(4.)
                .rounded(0.)
                .bg(rgb(0x224466))
                .child(
                    submenu("More", child_open.clone())
                        .id("child")
                        .size(100., 60.)
                        .p(4.)
                        .rounded(0.)
                        .bg(rgb(0xee3322))
                        .child(menu_item("Run").id("run").h(20.).p(0.).bg(rgb(0xee3322))),
                ),
            ),
    );
    let mut render = |ui: &Ui| {
        ui.prepare_frame();
        let report = ui.scene.borrow_mut().flush();
        gpu.render(&ui.scene.borrow(), &report.damage).unwrap();
        let partial = gpu.readback().unwrap();
        gpu.render(&ui.scene.borrow(), &[Rect::new(0., 0., 400., 240.)])
            .unwrap();
        assert!(
            partial == gpu.readback().unwrap(),
            "submenu damage must match a full repaint"
        );
        (partial, report.layout_nodes)
    };
    open.set(true);
    render(&ui);
    ui.dispatch(InputEvent::KeyDown {
        key: Key::ArrowRight,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    let (shown, _) = render(&ui);
    assert!(open.get() && child_open.get());
    assert_eq!(ui.input.focused(), view.find("run"));
    let child = view.find("child").unwrap();
    let parent = view.find("parent").unwrap();
    let bounds = ui.scene.borrow().bounds(child);
    let parent_bounds = ui.scene.borrow().bounds(parent);
    assert!(bounds.x >= parent_bounds.x + parent_bounds.width);
    let pixel = |pixels: &[u8], rect: Rect| {
        let i = ((rect.y as usize + 50) * 400 + rect.x as usize + 90) * 4;
        pixels[i..i + 4].to_vec()
    };
    assert_eq!(pixel(&shown, bounds), [238, 51, 34, 255]);
    movement.set(200.);
    let (flipped, layout) = render(&ui);
    assert_eq!(layout, 0);
    let bounds = ui.scene.borrow().bounds(child);
    let parent_bounds = ui.scene.borrow().bounds(parent);
    assert!(bounds.x + bounds.width <= parent_bounds.x);
    assert_eq!(pixel(&flipped, bounds), [238, 51, 34, 255]);
    ui.dispatch(InputEvent::KeyDown {
        key: Key::ArrowLeft,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    render(&ui);
    assert!(open.get());
    assert!(!child_open.get());
    ui.dispatch(InputEvent::KeyDown {
        key: Key::Escape,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    render(&ui);
    assert!(!open.get());
}

#[test]
fn wrapped_editor_pixels_follow_allocation_selection_and_preedit() {
    let _gpu_fixture = gpu_fixture();
    use zgx::{
        compose::prelude::*,
        input::{InputEvent, Key, Modifiers},
        text_layout::{FontFamily, FontStyle, TextLayout},
        widgets::Ui,
    };
    let mut gpu = GpuRenderer::new(320, 180).unwrap();
    let mut ui = Ui::new(320., 180.);
    let fonts = gpu.text_system();
    ui.scene.borrow_mut().set_font_text_shaper(
        move |text: &str, size, width, font: &FontStyle| -> Box<dyn TextLayout> {
            Box::new(zgx_gpu::text::ShapedText::with_font(
                &mut fonts.borrow_mut(),
                text,
                size,
                width,
                font,
            ))
        },
    );
    let original = "Alpha bravo charlie delta echo foxtrot golf hotel india juliet kilo lima";
    let value = ui.signal(original.to_owned());
    let width = ui.signal(180.);
    let wrapped = ui.signal(true);
    let styled_width = width.clone();
    let styled_wrap = wrapped.clone();
    let mounted = ui.mount(
        column().size(320., 180.).p(8.).bg(rgb(0x101820)).child(
            text_area("Wrapped", value.clone())
                .id("editor")
                .h(140.)
                .p(8.)
                .rounded(0.)
                .bg(rgb(0x203040))
                .text_color(rgb(0xff4020))
                .text_size(18.)
                .font_family(FontFamily::Monospace)
                .reactive_style(move || {
                    Styles::new()
                        .w(styled_width.get())
                        .text_wrap(styled_wrap.get())
                }),
        ),
    );
    let render = |ui: &Ui, gpu: &mut GpuRenderer| {
        ui.prepare_frame();
        let mut scene = ui.scene.borrow_mut();
        let damage = scene.flush().damage;
        gpu.render(&scene, &damage).unwrap();
        let partial = gpu.readback().unwrap();
        gpu.render(&scene, &[Rect::new(0., 0., 320., 180.)])
            .unwrap();
        assert!(
            partial == gpu.readback().unwrap(),
            "wrapped editor damage must match full repaint"
        );
        partial
    };
    let initial = render(&ui, &mut gpu);
    let red_rows = |pixels: &[u8]| {
        (0..180)
            .filter(|y| {
                pixels[y * 320 * 4..(y + 1) * 320 * 4]
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|p| p[0] > 200 && p[1] < 100 && p[2] < 80)
            })
            .count()
    };
    wrapped.set(false);
    let unwrapped = render(&ui, &mut gpu);
    assert!(
        red_rows(&initial) > red_rows(&unwrapped) * 2,
        "wrapping must paint multiple actual glyph lines"
    );
    wrapped.set(true);
    assert!(render(&ui, &mut gpu) == initial);
    ui.input.focus(&ui.scene, mounted.find("editor"));
    let editor = ui.focused_editor().unwrap();
    ui.dispatch(InputEvent::KeyDown {
        key: Key::End,
        modifiers: Modifiers {
            control: !cfg!(target_os = "macos"),
            meta: cfg!(target_os = "macos"),
            ..Default::default()
        },
        repeat: false,
    });
    for w in [180., 112., 280., 180.] {
        width.set(w);
        let pixels = render(&ui, &mut gpu);
        let scene = ui.scene.borrow();
        let bounds = scene.bounds(editor.node);
        let caret = scene.bounds(editor.caret);
        assert!(caret.x >= bounds.x + 8. && caret.x + caret.width <= bounds.x + bounds.width - 7.);
        assert!(
            caret.y >= bounds.y + 8. && caret.y + caret.height <= bounds.y + bounds.height - 7.
        );
        let at =
            (((caret.y + caret.height / 2.).floor() as usize) * 320 + caret.x.round() as usize) * 4;
        assert!(
            pixels[at] > 200 && pixels[at + 1] < 100,
            "visible caret must track wrapped geometry"
        );
    }
    assert_eq!(
        value.get(),
        original,
        "soft wrapping never inserts line breaks"
    );
    ui.dispatch(InputEvent::KeyDown {
        key: Key::Character("a".into()),
        modifiers: Modifiers {
            control: !cfg!(target_os = "macos"),
            meta: cfg!(target_os = "macos"),
            ..Default::default()
        },
        repeat: false,
    });
    let selected = render(&ui, &mut gpu);
    assert!(
        selected
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[0].abs_diff(48) <= 1 && p[1].abs_diff(81) <= 1 && p[2].abs_diff(127) <= 1)
    );
    let preedit = "caf\u{e9} composed words across wrapped lines";
    ui.dispatch(InputEvent::ImePreedit {
        text: preedit.into(),
        cursor: Some((preedit.len(), preedit.len())),
    });
    render(&ui, &mut gpu);
    assert_eq!(value.get(), original);
    ui.dispatch(InputEvent::ImeCommit(preedit.into()));
    render(&ui, &mut gpu);
    assert_eq!(value.get(), preedit);
    value.set("trailing newline\n".into());
    ui.dispatch(InputEvent::KeyDown {
        key: Key::End,
        modifiers: Modifiers {
            control: !cfg!(target_os = "macos"),
            meta: cfg!(target_os = "macos"),
            ..Default::default()
        },
        repeat: false,
    });
    let pixels = render(&ui, &mut gpu);
    let scene = ui.scene.borrow();
    let bounds = scene.bounds(editor.node);
    let caret = scene.bounds(editor.caret);
    assert!(caret.y >= bounds.y + 8. + caret.height);
    let at =
        (((caret.y + caret.height / 2.).floor() as usize) * 320 + caret.x.round() as usize) * 4;
    assert!(
        pixels[at] > 200 && pixels[at + 1] < 100,
        "trailing empty line must retain a visible caret"
    );
}

#[test]
fn inherited_line_height_reflows_glyphs_and_editor_without_rasterizing_again() {
    let _gpu_fixture = gpu_fixture();
    use zgx::{
        compose::prelude::*,
        input::{InputEvent, Key, Modifiers},
        text_layout::{FontFamily, FontStyle, TextLayout},
        widgets::Ui,
    };
    let mut gpu = GpuRenderer::new(320, 300).unwrap();
    let mut ui = Ui::new(320., 300.);
    let fonts = gpu.text_system();
    ui.scene.borrow_mut().set_font_text_shaper(
        move |text: &str, size, width, font: &FontStyle| -> Box<dyn TextLayout> {
            Box::new(zgx_gpu::text::ShapedText::with_font(
                &mut fonts.borrow_mut(),
                text,
                size,
                width,
                font,
            ))
        },
    );
    let pitch = ui.signal(None::<f32>);
    let read_pitch = pitch.clone();
    let value = ui.signal("Ag\nAg\nAg".to_owned());
    let mounted = ui.mount(
        column()
            .size(320., 300.)
            .p(8.)
            .gap(8.)
            .bg(rgb(0x101820))
            .text_color(rgb(0xff4020))
            .text_size(20.)
            .font_family(FontFamily::Monospace)
            .reactive_style(move || match read_pitch.get() {
                Some(height) => Styles::new().line_height(height),
                None => Styles::new().line_height_normal(),
            })
            .child(text("Ag\nAg\nAg").id("label"))
            .child(
                text_area("Editor", value.clone())
                    .id("editor")
                    .size(280., 140.)
                    .p(8.)
                    .rounded(0.)
                    .bg(rgb(0x203040)),
            ),
    );
    ui.input.focus(&ui.scene, mounted.find("editor"));
    ui.dispatch(InputEvent::KeyDown {
        key: Key::Home,
        modifiers: Modifiers {
            control: !cfg!(target_os = "macos"),
            meta: cfg!(target_os = "macos"),
            ..Default::default()
        },
        repeat: false,
    });
    ui.dispatch(InputEvent::KeyDown {
        key: Key::ArrowDown,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    let editor = ui.focused_editor().unwrap();
    let selection = editor.editor.borrow().selection();
    let render = |ui: &Ui, gpu: &mut GpuRenderer| {
        ui.prepare_frame();
        let mut scene = ui.scene.borrow_mut();
        let damage = scene.flush().damage;
        let stats = gpu.render(&scene, &damage).unwrap();
        let pixels = gpu.readback().unwrap();
        gpu.render(&scene, &[Rect::new(0., 0., 320., 300.)])
            .unwrap();
        assert!(
            pixels == gpu.readback().unwrap(),
            "line-height damage, including tight outer clipping, must match full repaint"
        );
        (pixels, stats)
    };
    let (initial, _) = render(&ui, &mut gpu);
    let mut previous = initial.clone();
    for (height, expected) in [(Some(40.), 40.), (Some(12.), 12.), (None, 28.)] {
        pitch.set(height);
        let (pixels, stats) = render(&ui, &mut gpu);
        assert!(
            pixels != previous,
            "line-height changes actual painted glyph placement"
        );
        assert_eq!(
            stats.glyph_uploads, 0,
            "line-height changes reuse existing rasterized glyphs"
        );
        let scene = ui.scene.borrow();
        assert_eq!(
            scene.bounds(mounted.find("label").unwrap()).height,
            expected * 3.
        );
        let caret = scene.bounds(editor.caret);
        let bounds = scene.bounds(editor.node);
        assert_eq!(caret.height, expected);
        assert!(
            (caret.y - bounds.y - 8. - expected).abs() < 0.01,
            "pitch={expected} caret={caret:?} root={bounds:?} selection={selection:?}"
        );
        let at =
            (((caret.y + caret.height / 2.).floor() as usize) * 320 + caret.x.round() as usize) * 4;
        assert!(pixels[at] > 200 && pixels[at + 1] < 100);
        assert_eq!(editor.editor.borrow().selection(), selection);
        assert_eq!(value.get(), "Ag\nAg\nAg");
        previous = pixels;
    }
    assert!(
        previous == initial,
        "normal reset restores exact glyph and caret pixels"
    );
}

#[test]
fn unchanged_native_editor_reuses_shaping_and_preserves_damage_pixels() {
    let _gpu_fixture = gpu_fixture();
    use std::{cell::Cell, rc::Rc};
    use zgx::{
        compose::prelude::*,
        input::{InputEvent, Key, Modifiers},
        text_layout::{FontStyle, TextLayout},
        widgets::Ui,
    };
    let mut gpu = GpuRenderer::new(320, 180).unwrap();
    let mut ui = Ui::new(320., 180.);
    let calls = Rc::new(Cell::new(0));
    let count = calls.clone();
    let fonts = gpu.text_system();
    ui.scene.borrow_mut().set_font_text_shaper(
        move |text: &str, size, width, font: &FontStyle| -> Box<dyn TextLayout> {
            count.set(count.get() + 1);
            Box::new(zgx_gpu::text::ShapedText::with_font(
                &mut fonts.borrow_mut(),
                text,
                size,
                width,
                font,
            ))
        },
    );
    let value = ui.signal("Alpha e\u{301} emoji 👩‍💻\nSecond line\nThird line".to_owned());
    let mounted = ui.mount(
        text_area("Editor", value.clone())
            .size(300., 160.)
            .p(8.)
            .text_size(18.)
            .text_wrap(true)
            .bg(rgb(0x203040)),
    );
    ui.prepare_frame();
    ui.input.focus(&ui.scene, Some(mounted.node()));
    ui.prepare_frame();
    let editor = ui.focused_editor().unwrap();
    editor.editor.borrow_mut().set_selection(0, 0);
    editor.refresh();
    ui.prepare_frame();
    {
        let mut scene = ui.scene.borrow_mut();
        let damage = scene.flush().damage;
        gpu.render(&scene, &damage).unwrap();
    }
    calls.set(0);
    for key in [Key::End, Key::ArrowDown, Key::Home, Key::ArrowUp] {
        ui.dispatch(InputEvent::KeyDown {
            key,
            modifiers: Modifiers::default(),
            repeat: false,
        });
        ui.prepare_frame();
        let mut scene = ui.scene.borrow_mut();
        let damage = scene.flush().damage;
        gpu.render(&scene, &damage).unwrap();
        let partial = gpu.readback().unwrap();
        gpu.render(&scene, &[Rect::new(0., 0., 320., 180.)])
            .unwrap();
        assert_eq!(
            partial,
            gpu.readback().unwrap(),
            "cached editor damage differs from full repaint"
        );
    }
    assert_eq!(
        calls.get(),
        0,
        "navigation must reuse the native editor layout"
    );
    assert_eq!(editor.editor.borrow().selection().focus, 0);
    assert_eq!(editor.editor.borrow().text(), value.get());
}

#[test]
fn percentage_panel_resize_updates_damage_and_keeps_idle_layout_cached() {
    let _gpu_fixture = gpu_fixture();
    use zgx::{compose::prelude::*, widgets::Ui};
    let mut gpu = GpuRenderer::new(320, 200).unwrap();
    let mut ui = Ui::new(320., 200.);
    let width = ui.signal(240.);
    let read_width = width.clone();
    let mounted = ui.mount(
        overlay().w_full().h_full().bg(rgb(0x102030)).child(
            overlay()
                .h(120.)
                .p(10.)
                .bg(rgb(0x304050))
                .reactive_style(move || Styles::new().w(read_width.get()))
                .child(
                    overlay()
                        .id("half")
                        .w_percent(50.)
                        .h_full()
                        .bg(rgb(0xff0000)),
                ),
        ),
    );
    let half = mounted.find("half").unwrap();
    for (parent_width, child_width) in [(240., 110.), (160., 70.), (280., 130.)] {
        width.set(parent_width);
        ui.prepare_frame();
        let mut scene = ui.scene.borrow_mut();
        assert_eq!(scene.bounds(half), Rect::new(10., 10., child_width, 100.));
        let damage = scene.flush().damage;
        gpu.render(&scene, &damage).unwrap();
        let partial = gpu.readback().unwrap();
        assert_eq!(
            partial
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|p| p[0] == 255 && p[1] == 0 && p[2] == 0)
                .count(),
            child_width as usize * 100
        );
        gpu.render(&scene, &[Rect::new(0., 0., 320., 200.)])
            .unwrap();
        assert_eq!(partial, gpu.readback().unwrap());
        assert!(scene.flush().damage.is_empty());
    }
}

#[test]
fn inherited_letter_spacing_updates_editor_and_pixels_without_new_glyph_uploads() {
    let _gpu_fixture = gpu_fixture();
    use zgx::{
        compose::prelude::*,
        text_layout::{FontFamily, FontStyle, TextLayout},
        widgets::Ui,
    };
    let mut gpu = GpuRenderer::new(320, 200).unwrap();
    let mut ui = Ui::new(320., 200.);
    let fonts = gpu.text_system();
    ui.scene.borrow_mut().set_font_text_shaper(
        move |text: &str, size, width, font: &FontStyle| -> Box<dyn TextLayout> {
            Box::new(zgx_gpu::text::ShapedText::with_font(
                &mut fonts.borrow_mut(),
                text,
                size,
                width,
                font,
            ))
        },
    );
    let spacing = ui.signal(0.);
    let read_spacing = spacing.clone();
    let model = ui.signal("ABCDEF".to_owned());
    let mounted = ui.mount(
        column()
            .size(320., 200.)
            .p(8.)
            .gap(8.)
            .items_start()
            .bg(rgb(0x102030))
            .text_color(rgb(0xff4020))
            .text_size(20.)
            .font_family(FontFamily::Monospace)
            .reactive_style(move || Styles::new().letter_spacing(read_spacing.get()))
            .child(text("ABCDEF").id("label"))
            .child(
                text_area("Editor", model.clone())
                    .id("editor")
                    .size(280., 80.)
                    .p(8.),
            ),
    );
    ui.prepare_frame();
    ui.input.focus(&ui.scene, mounted.find("editor"));
    let editor = ui.focused_editor().unwrap();
    editor.editor.borrow_mut().set_selection(3, 3);
    editor.refresh();
    ui.prepare_frame();
    let label = mounted.find("label").unwrap();
    let baseline_width = ui.scene.borrow().bounds(label).width;
    let baseline_caret = ui.scene.borrow().bounds(editor.caret).x;
    let mut initial = Vec::new();
    for (index, value) in [0., 3., -1., 0.].into_iter().enumerate() {
        spacing.set(value);
        ui.prepare_frame();
        let mut scene = ui.scene.borrow_mut();
        assert!((scene.bounds(label).width - baseline_width - 6. * value).abs() < 0.01);
        assert!((scene.bounds(editor.caret).x - baseline_caret - 3. * value).abs() < 0.01);
        assert_eq!(editor.editor.borrow().selection().focus, 3);
        assert_eq!(editor.editor.borrow().text(), "ABCDEF");
        let damage = scene.flush().damage;
        let stats = gpu.render(&scene, &damage).unwrap();
        let partial = gpu.readback().unwrap();
        gpu.render(&scene, &[Rect::new(0., 0., 320., 200.)])
            .unwrap();
        assert_eq!(partial, gpu.readback().unwrap());
        if index == 0 {
            initial = partial;
        } else {
            assert_eq!(
                stats.glyph_uploads, 0,
                "tracking changes glyph placement, not glyph bitmaps"
            );
            if value == 0. {
                assert_eq!(partial, initial);
            } else {
                assert_ne!(partial, initial);
            }
        }
    }
}

#[test]
fn object_fit_pixels_crop_center_and_reuse_uploaded_image_across_transitions() {
    let _gpu_fixture = gpu_fixture();
    use std::sync::Arc;
    use zgx::{compose::prelude::*, image::ImageData, widgets::Ui};
    let colors = [
        [220, 30, 40, 255],
        [30, 210, 60, 255],
        [30, 70, 220, 255],
        [230, 200, 20, 255],
    ];
    let pixels: Vec<u8> = (0..20)
        .flat_map(|_| (0..40).flat_map(|x| colors[x / 10]))
        .collect();
    let source = Arc::new(ImageData::new(40, 20, pixels).unwrap());
    let mut gpu = GpuRenderer::new(112, 104).unwrap();
    let mut ui = Ui::new(112., 104.);
    let fit = ui.signal(ObjectFit::Fill);
    let size = ui.signal((88., 88.));
    let bitmap = ui.signal(source);
    let read_fit = fit.clone();
    let read_size = size.clone();
    let read_bitmap = bitmap.clone();
    ui.mount(
        image_signal("Pattern", move || read_bitmap.get())
            .p(4.)
            .bg(rgb(0x102030))
            .reactive_style(move || {
                let (w, h) = read_size.get();
                Styles::new().size(w, h).object_fit(read_fit.get())
            }),
    );
    let background = [16, 32, 48, 255];
    for (index, (mode, allocation, colored_area, samples)) in [
        (
            ObjectFit::Fill,
            88.,
            6400,
            vec![(10, 10, colors[0]), (76, 10, colors[3])],
        ),
        (
            ObjectFit::Contain,
            88.,
            3200,
            vec![
                (10, 10, background),
                (10, 30, colors[0]),
                (76, 30, colors[3]),
            ],
        ),
        (
            ObjectFit::Cover,
            88.,
            6400,
            vec![(10, 10, colors[1]), (76, 10, colors[2])],
        ),
        (
            ObjectFit::None,
            88.,
            800,
            vec![
                (10, 10, background),
                (26, 36, colors[0]),
                (60, 36, colors[3]),
            ],
        ),
        (
            ObjectFit::ScaleDown,
            88.,
            800,
            vec![(10, 10, background), (26, 36, colors[0])],
        ),
        (
            ObjectFit::ScaleDown,
            28.,
            200,
            vec![(5, 5, background), (5, 12, colors[0]), (22, 12, colors[3])],
        ),
        (
            ObjectFit::Cover,
            28.,
            400,
            vec![(5, 5, colors[1]), (22, 5, colors[2])],
        ),
        (
            ObjectFit::Fill,
            88.,
            6400,
            vec![(10, 10, colors[0]), (76, 10, colors[3])],
        ),
    ]
    .into_iter()
    .enumerate()
    {
        fit.set(mode);
        size.set((allocation, allocation));
        ui.prepare_frame();
        let frame = ui.scene.borrow_mut().flush();
        let stats = gpu.render(&ui.scene.borrow(), &frame.damage).unwrap();
        assert_eq!(
            stats.image_uploads,
            usize::from(index == 0),
            "fit stage {index}"
        );
        assert_eq!(gpu.debug_cache_stats().image_textures, 1);
        let partial = gpu.readback().unwrap();
        let full = gpu
            .render(&ui.scene.borrow(), &[Rect::new(0., 0., 112., 104.)])
            .unwrap();
        assert_eq!(full.image_uploads, 0);
        assert_eq!(partial, gpu.readback().unwrap(), "damage stage {index}");
        let color_count = partial
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[3] == 255 && **pixel != background)
            .count();
        assert_eq!(
            color_count, colored_area,
            "visible image area stage {index}"
        );
        for (x, y, expected) in samples {
            assert_eq!(
                &partial[(y * 112 + x) * 4..(y * 112 + x) * 4 + 4],
                &expected,
                "stage {index} at{x},{y}"
            );
        }
        for (x, y) in [
            (2, 10),
            (10, 2),
            (allocation as usize - 2, 10),
            (10, allocation as usize - 2),
        ] {
            assert_eq!(
                &partial[(y * 112 + x) * 4..(y * 112 + x) * 4 + 4],
                &background,
                "padding stage {index}"
            );
        }
    }
    bitmap.set(Arc::new(
        ImageData::new(20, 40, [180, 40, 200, 255].repeat(20 * 40)).unwrap(),
    ));
    fit.set(ObjectFit::Contain);
    ui.prepare_frame();
    let frame = ui.scene.borrow_mut().flush();
    assert_eq!(
        gpu.render(&ui.scene.borrow(), &frame.damage)
            .unwrap()
            .image_uploads,
        1
    );
    let partial = gpu.readback().unwrap();
    assert_eq!(
        partial
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| **p == [180, 40, 200, 255])
            .count(),
        3200
    );
    assert_eq!(
        &partial[(40 * 112 + 10) * 4..(40 * 112 + 10) * 4 + 4],
        &background
    );
    gpu.render(&ui.scene.borrow(), &[Rect::new(0., 0., 112., 104.)])
        .unwrap();
    assert_eq!(partial, gpu.readback().unwrap());
    assert_eq!(gpu.debug_cache_stats().image_textures, 1);
}

#[test]
fn invisible_isolated_images_defer_upload_and_reveal_cached_pixels_correctly() {
    let _gpu_fixture = gpu_fixture();
    use std::sync::Arc;
    use zgx::image::ImageData;
    let mut gpu = GpuRenderer::new(32, 24).unwrap();
    let mut scene = Scene::new(32., 24.);
    scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
    let group = scene.append(
        scene.root(),
        NodeKind::Container(Layout::Overlay),
        fixed(16., 16.),
    );
    scene.set_isolated(group, true);
    let nested = scene.append(group, NodeKind::Container(Layout::Overlay), fixed(16., 16.));
    scene.set_isolated(nested, true);
    let red = Arc::new(ImageData::new(2, 2, [220, 30, 40, 255].repeat(4)).unwrap());
    let blue = Arc::new(ImageData::new(2, 2, [30, 80, 220, 255].repeat(4)).unwrap());
    let bitmap = scene.append(nested, NodeKind::Image(red), fixed(16., 16.));
    for (index, opacity) in [0., 1., 0., 1.].into_iter().enumerate() {
        if index == 2 {
            scene.set_kind(bitmap, NodeKind::Image(blue.clone()));
        }
        scene.set_effects(
            group,
            Effects {
                opacity,
                ..Default::default()
            },
        );
        let frame = scene.flush();
        let stats = gpu.render(&scene, &frame.damage).unwrap();
        assert_eq!(
            stats.image_uploads,
            usize::from(opacity > 0.),
            "stage{index}"
        );
        if opacity == 0. {
            assert_eq!(stats.layer_repaints, 0);
            if index == 0 {
                assert_eq!(gpu.debug_cache_stats().layer_textures, 0);
            }
        }
        let partial = gpu.readback().unwrap();
        gpu.render(&scene, &[Rect::new(0., 0., 32., 24.)]).unwrap();
        assert_eq!(partial, gpu.readback().unwrap());
        let expected = match index {
            1 => [220, 30, 40, 255],
            3 => [30, 80, 220, 255],
            _ => [0, 0, 0, 0],
        };
        assert_eq!(&partial[(8 * 32 + 8) * 4..(8 * 32 + 8) * 4 + 4], &expected);
    }
}

#[test]
fn zero_opacity_backdrop_blur_preserves_pixels_before_and_after_reveal() {
    let _gpu_fixture = gpu_fixture();
    for isolated in [false, true] {
        let mut gpu = GpuRenderer::new(64, 32).unwrap();
        let mut scene = Scene::new(64., 32.);
        scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
        scene.append(
            scene.root(),
            NodeKind::Rect(Color(0, 0, 0, 255)),
            fixed(64., 32.),
        );
        scene.append(
            scene.root(),
            NodeKind::Rect(Color(255, 255, 255, 255)),
            fixed(32., 32.),
        );
        let damage = scene.flush().damage;
        gpu.render(&scene, &damage).unwrap();
        let baseline = gpu.readback().unwrap();
        let glass = scene.append(
            scene.root(),
            NodeKind::Container(Layout::Overlay),
            fixed(64., 32.),
        );
        scene.set_isolated(glass, isolated);
        scene.append(glass, NodeKind::Rect(Color(255, 0, 0, 40)), fixed(64., 32.));
        for opacity in [0., 1., 0.] {
            scene.set_effects(
                glass,
                Effects {
                    opacity,
                    blur_radius: 4.,
                    ..Default::default()
                },
            );
            let damage = scene.flush().damage;
            let stats = gpu.render(&scene, &damage).unwrap();
            let partial = gpu.readback().unwrap();
            gpu.render(&scene, &[Rect::new(0., 0., 64., 32.)]).unwrap();
            assert_eq!(
                partial,
                gpu.readback().unwrap(),
                "isolated={isolated} opacity={opacity}"
            );
            if opacity == 0. {
                assert_eq!(
                    partial, baseline,
                    "hidden blur altered backdrop, isolated={isolated}"
                );
                assert_eq!(stats.layer_repaints, 0);
            } else {
                assert_ne!(partial, baseline);
                let boundary = partial[(16 * 64 + 32) * 4 + 1];
                assert!(
                    boundary > 20 && boundary < 220,
                    "visible blur must cross edge: {boundary}, isolated={isolated}"
                );
            }
        }
    }
}

#[test]
fn hidden_oversized_descendant_does_not_inflate_visible_isolated_layer() {
    let _gpu_fixture = gpu_fixture();
    let mut gpu = GpuRenderer::new(48, 32).unwrap();
    let mut scene = Scene::new(48., 32.);
    let group = scene.append(
        scene.root(),
        NodeKind::Container(Layout::Overlay),
        fixed(24., 24.),
    );
    scene.set_isolated(group, true);
    scene.append(
        group,
        NodeKind::Rect(Color(30, 80, 220, 255)),
        fixed(24., 24.),
    );
    let hidden = scene.append(
        group,
        NodeKind::Container(Layout::Overlay),
        fixed(8192., 8192.),
    );
    scene.set_effects(
        hidden,
        Effects {
            opacity: 0.,
            ..Default::default()
        },
    );
    let child = scene.append(
        hidden,
        NodeKind::Rect(Color(220, 30, 40, 255)),
        fixed(8192., 8192.),
    );
    let damage = scene.flush().damage;
    let stats = gpu
        .render(&scene, &damage)
        .expect("hidden descendants must not consume layer allocation budget");
    assert_eq!(stats.layer_texture_allocations, 1);
    // 26x26 content, allocated in 64-texel steps.
    assert_eq!(gpu.debug_cache_stats().layer_bytes, 64 * 64 * 4);
    let baseline = gpu.readback().unwrap();
    assert_eq!(
        &baseline[(8 * 48 + 8) * 4..(8 * 48 + 8) * 4 + 4],
        &[30, 80, 220, 255]
    );
    scene.set_style(hidden, fixed(12., 12.));
    scene.set_style(child, fixed(12., 12.));
    scene.set_effects(hidden, Effects::default());
    let damage = scene.flush().damage;
    gpu.render(&scene, &damage).unwrap();
    let partial = gpu.readback().unwrap();
    assert_eq!(
        &partial[(8 * 48 + 8) * 4..(8 * 48 + 8) * 4 + 4],
        &[220, 30, 40, 255]
    );
    assert_eq!(
        &partial[(18 * 48 + 18) * 4..(18 * 48 + 18) * 4 + 4],
        &[30, 80, 220, 255]
    );
    assert_eq!(gpu.debug_cache_stats().layer_bytes, 64 * 64 * 4);
    gpu.render(&scene, &[Rect::new(0., 0., 48., 32.)]).unwrap();
    assert_eq!(partial, gpu.readback().unwrap());
}

#[test]
fn nonfinite_effects_normalize_to_stable_pixels_and_repeated_writes_stay_idle() {
    let _gpu_fixture = gpu_fixture();
    for isolated in [false, true] {
        let mut gpu = GpuRenderer::new(48, 32).unwrap();
        let mut scene = Scene::new(48., 32.);
        scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
        scene.append(
            scene.root(),
            NodeKind::Rect(Color(20, 40, 80, 255)),
            fixed(48., 32.),
        );
        let foreground = scene.append(
            scene.root(),
            NodeKind::Rect(Color(220, 30, 40, 255)),
            fixed(24., 24.),
        );
        scene.set_isolated(foreground, isolated);
        let damage = scene.flush().damage;
        gpu.render(&scene, &damage).unwrap();
        let baseline = gpu.readback().unwrap();
        for invalid in [
            Effects {
                opacity: f32::INFINITY,
                blur_radius: -3.,
                edge_fade: -4.,
                filter_blur: 0.,
                brightness: 1.,
                contrast: 1.,
                saturate: 1.,
            },
            Effects {
                opacity: f32::NAN,
                blur_radius: f32::INFINITY,
                edge_fade: f32::NEG_INFINITY,
                filter_blur: 0.,
                brightness: 1.,
                contrast: 1.,
                saturate: 1.,
            },
            Effects {
                opacity: f32::NAN,
                blur_radius: f32::NAN,
                edge_fade: f32::INFINITY,
                filter_blur: 0.,
                brightness: 1.,
                contrast: 1.,
                saturate: 1.,
            },
            Effects {
                opacity: 1.,
                blur_radius: f32::NEG_INFINITY,
                edge_fade: f32::NAN,
                filter_blur: 0.,
                brightness: 1.,
                contrast: 1.,
                saturate: 1.,
            },
        ] {
            scene.set_effects(
                foreground,
                Effects {
                    opacity: 0.5,
                    blur_radius: 2.,
                    edge_fade: 2.,
                    filter_blur: 0.,
                    brightness: 1.,
                    contrast: 1.,
                    saturate: 1.,
                },
            );
            let damage = scene.flush().damage;
            gpu.render(&scene, &damage).unwrap();
            assert_ne!(gpu.readback().unwrap(), baseline);
            scene.set_effects(foreground, invalid);
            let damage = scene.flush().damage;
            gpu.render(&scene, &damage).unwrap();
            let partial = gpu.readback().unwrap();
            assert!(
                partial == baseline,
                "normalized fallback isolated={isolated}"
            );
            gpu.render(&scene, &[Rect::new(0., 0., 48., 32.)]).unwrap();
            assert_eq!(partial, gpu.readback().unwrap());
            scene.set_effects(foreground, invalid);
            let idle = scene.flush();
            assert!(
                idle.is_idle(),
                "normalized invalid equality isolated={isolated}"
            );
            let stats = gpu.render(&scene, &idle.damage).unwrap();
            assert_eq!(stats.draw_calls, 0);
            assert_eq!(stats.image_uploads, 0);
            assert_eq!(stats.glyph_uploads, 0);
            assert_eq!(stats.layer_repaints, 0);
            assert_eq!(baseline, gpu.readback().unwrap());
        }
    }
}

#[test]
fn nonfinite_translations_restore_pixels_without_repainting_retained_layers() {
    let _gpu_fixture = gpu_fixture();
    for isolated in [false, true] {
        let mut gpu = GpuRenderer::new(64, 40).unwrap();
        let mut scene = Scene::new(64., 40.);
        scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
        scene.append(
            scene.root(),
            NodeKind::Rect(Color(20, 40, 80, 255)),
            fixed(64., 40.),
        );
        let node = scene.append(
            scene.root(),
            NodeKind::Rect(Color(220, 30, 40, 255)),
            fixed(16., 12.),
        );
        scene.set_isolated(node, isolated);
        let frame = scene.flush();
        gpu.render(&scene, &frame.damage).unwrap();
        let baseline = gpu.readback().unwrap();
        for (invalid, expected) in [
            (
                Transform {
                    x: f32::NAN,
                    y: f32::INFINITY,
                },
                Transform { x: 0., y: 0. },
            ),
            (
                Transform {
                    x: f32::NEG_INFINITY,
                    y: 7.,
                },
                Transform { x: 0., y: 7. },
            ),
            (Transform { x: 9., y: f32::NAN }, Transform { x: 9., y: 0. }),
        ] {
            scene.set_transform(node, Transform { x: 28., y: 20. });
            let frame = scene.flush();
            gpu.render(&scene, &frame.damage).unwrap();
            scene.set_transform(node, invalid);
            let frame = scene.flush();
            assert_eq!(frame.layout_nodes, 0);
            let stats = gpu.render(&scene, &frame.damage).unwrap();
            assert_eq!(stats.layer_repaints, 0);
            assert_eq!(stats.layer_texture_allocations, 0);
            let partial = gpu.readback().unwrap();
            gpu.render(&scene, &[Rect::new(0., 0., 64., 40.)]).unwrap();
            assert!(
                partial == gpu.readback().unwrap(),
                "partial/full isolated={isolated}"
            );
            if expected.x == 0. && expected.y == 0. {
                assert!(partial == baseline);
            }
            let x = expected.x as usize + 4;
            let y = expected.y as usize + 4;
            assert_eq!(
                &partial[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4],
                &[220, 30, 40, 255]
            );
            assert_eq!(
                &partial[(24 * 64 + 32) * 4..(24 * 64 + 32) * 4 + 4],
                &[20, 40, 80, 255]
            );
            scene.set_transform(node, invalid);
            let idle = scene.flush();
            assert!(idle.is_idle());
            assert_eq!(gpu.render(&scene, &idle.damage).unwrap().draw_calls, 0);
        }
    }
}

fn assert_blur_matches_fresh(
    gpu: &GpuRenderer,
    scene: &Scene,
    width: u32,
    height: u32,
    scale: f32,
) {
    let partial = gpu.readback().unwrap();
    let mut fresh = GpuRenderer::new_with_context(width, height, &gpu.context()).unwrap();
    fresh.set_scale_factor(scale);
    fresh.render(scene, &[scene.bounds(scene.root())]).unwrap();
    assert!(
        partial == fresh.readback().unwrap(),
        "bounded blur reconstruction differs from a fresh full frame"
    );
}

#[test]
fn bounded_blur_damage_distant_and_backdrop_halo_updates() {
    let _fixture = gpu_fixture();
    let mut gpu = GpuRenderer::new(240, 160).unwrap();
    let mut scene = Scene::new(240., 160.);
    scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
    scene.append(
        scene.root(),
        NodeKind::Rect(Color(20, 40, 60, 255)),
        fixed(240., 160.),
    );
    // Straddles the filter's left edge (x 36): half of it is backdrop.
    let backdrop = scene.append(
        scene.root(),
        NodeKind::Rect(Color(240, 220, 80, 255)),
        fixed(12., 12.),
    );
    scene.set_transform(backdrop, Transform { x: 30., y: 48. });
    // Just outside the filter: it samples nothing beyond its own bounds.
    let beside = scene.append(
        scene.root(),
        NodeKind::Rect(Color(90, 90, 200, 255)),
        fixed(4., 12.),
    );
    scene.set_transform(beside, Transform { x: 30., y: 64. });
    let blur = scene.append(
        scene.root(),
        NodeKind::Rect(Color(230, 40, 70, 35)),
        fixed(48., 48.),
    );
    scene.set_transform(blur, Transform { x: 36., y: 32. });
    scene.set_effects(
        blur,
        Effects {
            blur_radius: 4.,
            ..Effects::default()
        },
    );
    let distant = scene.append(
        scene.root(),
        NodeKind::Rect(Color(10, 200, 30, 180)),
        fixed(4., 4.),
    );
    scene.set_transform(distant, Transform { x: 220., y: 140. });
    let initial_damage = scene.flush().damage;
    gpu.render(&scene, &initial_damage).unwrap();
    scene.set_kind(distant, NodeKind::Rect(Color(200, 30, 10, 180)));
    let damage = scene.flush().damage;
    let stats = gpu.render(&scene, &damage).unwrap();
    assert_eq!(
        stats.damaged_pixels, 16,
        "distant paint must not replay a clean blur"
    );
    assert_eq!(stats.blur_passes, 0);
    assert_eq!(
        stats.render_passes, 1,
        "distant changes use one batched pass"
    );
    assert_blur_matches_fresh(&gpu, &scene, 240, 160, 1.);
    scene.set_kind(beside, NodeKind::Rect(Color(200, 90, 90, 255)));
    let damage = scene.flush().damage;
    let stats = gpu.render(&scene, &damage).unwrap();
    assert_eq!(
        stats.blur_passes, 0,
        "a change beside the filter must not re-blur it"
    );
    assert_blur_matches_fresh(&gpu, &scene, 240, 160, 1.);
    let before = gpu.readback().unwrap();
    scene.set_kind(backdrop, NodeKind::Rect(Color(10, 230, 200, 255)));
    let damage = scene.flush().damage;
    let stats = gpu.render(&scene, &damage).unwrap();
    assert!(
        stats.damaged_pixels < 240 * 160 / 2,
        "small blur halo should remain local: {stats:?}"
    );
    assert_blur_matches_fresh(&gpu, &scene, 240, 160, 1.);
    assert_eq!(stats.blur_passes, 3);
    assert!(stats.render_passes > stats.blur_passes);
    let after = gpu.readback().unwrap();
    let sample = (53 * 240 + 37) * 4;
    assert_ne!(
        &before[sample..sample + 4],
        &after[sample..sample + 4],
        "backdrop change under the filter must affect its blurred pixels"
    );
}

#[test]
fn bounded_blur_damage_cascades_through_overlapping_filters_without_feedback() {
    let _fixture = gpu_fixture();
    let mut gpu = GpuRenderer::new(320, 160).unwrap();
    let mut scene = Scene::new(320., 160.);
    scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
    scene.append(
        scene.root(),
        NodeKind::Rect(Color(15, 30, 45, 255)),
        fixed(320., 160.),
    );
    let backdrop = scene.append(
        scene.root(),
        NodeKind::Rect(Color(250, 220, 80, 255)),
        fixed(6., 24.),
    );
    scene.set_transform(backdrop, Transform { x: 28., y: 45. });
    for (x, color) in [
        (24., Color(210, 30, 40, 60)),
        (62., Color(20, 210, 50, 55)),
        (101., Color(30, 50, 230, 70)),
    ] {
        let glass = scene.append(scene.root(), NodeKind::Rect(color), fixed(48., 52.));
        scene.set_transform(glass, Transform { x, y: 32. });
        scene.set_effects(
            glass,
            Effects {
                blur_radius: 4.,
                ..Effects::default()
            },
        );
    }
    let overlay = scene.append(
        scene.root(),
        NodeKind::Rect(Color(230, 180, 40, 100)),
        fixed(100., 7.),
    );
    scene.set_transform(overlay, Transform { x: 35., y: 51. });
    let initial_damage = scene.flush().damage;
    gpu.render(&scene, &initial_damage).unwrap();
    for color in [
        Color(20, 230, 200, 255),
        Color(240, 30, 140, 255),
        Color(20, 230, 200, 255),
    ] {
        scene.set_kind(backdrop, NodeKind::Rect(color));
        let damage = scene.flush().damage;
        let stats = gpu.render(&scene, &damage).unwrap();
        assert!(
            stats.damaged_pixels < 320 * 160,
            "filter dependency closure should be bounded: {stats:?}"
        );
        assert_blur_matches_fresh(&gpu, &scene, 320, 160, 1.);
        gpu.render(&scene, &damage).unwrap();
        assert_blur_matches_fresh(&gpu, &scene, 320, 160, 1.);
    }
}

#[test]
fn bounded_blur_damage_respects_clipped_and_offscreen_filter_outputs() {
    let _fixture = gpu_fixture();
    let mut gpu = GpuRenderer::new(200, 120).unwrap();
    let mut scene = Scene::new(200., 120.);
    scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
    scene.append(
        scene.root(),
        NodeKind::Rect(Color(30, 45, 70, 255)),
        fixed(200., 120.),
    );
    let backdrop = scene.append(
        scene.root(),
        NodeKind::Rect(Color(240, 230, 180, 255)),
        fixed(24., 40.),
    );
    scene.set_transform(backdrop, Transform { x: 1., y: 25. });
    let clip = scene.append(
        scene.root(),
        NodeKind::Container(Layout::Overlay),
        Style {
            clip: true,
            ..fixed(45., 55.)
        },
    );
    scene.set_transform(clip, Transform { x: 0., y: 20. });
    let glass = scene.append(
        clip,
        NodeKind::Rect(Color(100, 150, 240, 75)),
        fixed(80., 70.),
    );
    scene.set_transform(glass, Transform { x: -22.5, y: -5.25 });
    scene.set_effects(
        glass,
        Effects {
            blur_radius: 5.5,
            ..Effects::default()
        },
    );
    let initial_damage = scene.flush().damage;
    gpu.render(&scene, &initial_damage).unwrap();
    scene.set_kind(backdrop, NodeKind::Rect(Color(180, 20, 40, 255)));
    let damage = scene.flush().damage;
    let stats = gpu.render(&scene, &damage).unwrap();
    assert!(stats.damaged_pixels < 200 * 120);
    assert_blur_matches_fresh(&gpu, &scene, 200, 120, 1.);
    scene.set_transform(glass, Transform { x: -10.25, y: 2.5 });
    let damage = scene.flush().damage;
    gpu.render(&scene, &damage).unwrap();
    assert_blur_matches_fresh(&gpu, &scene, 200, 120, 1.);
}

#[test]
fn bounded_blur_damage_fractional_scale_and_sigma_clamp_match_fresh_frames() {
    let _fixture = gpu_fixture();
    let mut gpu = GpuRenderer::new(360, 240).unwrap();
    for (scale, sigma) in [(1.5, 0.01), (1.5, 3.75), (2., 100_000.)] {
        gpu.set_scale_factor(scale);
        let width = 360. / scale;
        let height = 240. / scale;
        let mut scene = Scene::new(width, height);
        scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
        scene.append(
            scene.root(),
            NodeKind::Rect(Color(15, 30, 45, 255)),
            fixed(width, height),
        );
        let backdrop = scene.append(
            scene.root(),
            NodeKind::Rect(Color(250, 235, 205, 255)),
            fixed(7.25, 13.5),
        );
        scene.set_transform(backdrop, Transform { x: 43.25, y: 42.75 });
        let glass = scene.append(
            scene.root(),
            NodeKind::Rect(Color(80, 170, 225, 70)),
            fixed(41.25, 31.5),
        );
        scene.set_transform(glass, Transform { x: 45.5, y: 35.25 });
        scene.set_effects(
            glass,
            Effects {
                blur_radius: sigma,
                ..Effects::default()
            },
        );
        let initial_damage = scene.flush().damage;
        gpu.render(&scene, &initial_damage).unwrap();
        scene.set_kind(backdrop, NodeKind::Rect(Color(220, 40, 60, 255)));
        let damage = scene.flush().damage;
        let stats = gpu.render(&scene, &damage).unwrap();
        if sigma < 10. {
            assert!(stats.damaged_pixels < 360 * 240);
        }
        assert_blur_matches_fresh(&gpu, &scene, 360, 240, scale);
    }
}

#[test]
fn bounded_blur_damage_many_fractional_mutations_match_fresh_frames() {
    let _fixture = gpu_fixture();
    let mut gpu = GpuRenderer::new(160, 120).unwrap();
    gpu.set_scale_factor(1.25);
    let mut scene = Scene::new(128., 96.);
    scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
    scene.append(
        scene.root(),
        NodeKind::Rect(Color(12, 25, 38, 255)),
        fixed(128., 96.),
    );
    let backdrop = scene.append(
        scene.root(),
        NodeKind::Rect(Color(240, 190, 70, 220)),
        fixed(11.5, 21.25),
    );
    scene.set_transform(backdrop, Transform { x: 18.25, y: 28.5 });
    let group = scene.append(
        scene.root(),
        NodeKind::Container(Layout::Overlay),
        Style {
            clip: true,
            ..fixed(82.5, 63.75)
        },
    );
    scene.set_transform(group, Transform { x: 9.5, y: 8.25 });
    let glass = scene.append(
        group,
        NodeKind::Rect(Color(170, 60, 220, 60)),
        fixed(43.5, 37.75),
    );
    scene.set_transform(
        glass,
        Transform {
            x: 17.25,
            y: 10.125,
        },
    );
    scene.set_effects(
        glass,
        Effects {
            blur_radius: 4.5,
            ..Effects::default()
        },
    );
    let mut other = Some(scene.append(
        scene.root(),
        NodeKind::Rect(Color(20, 180, 150, 65)),
        fixed(34.25, 29.5),
    ));
    scene.set_transform(other.unwrap(), Transform { x: 54.25, y: 35.5 });
    scene.set_effects(
        other.unwrap(),
        Effects {
            blur_radius: 3.25,
            ..Effects::default()
        },
    );
    let foreground = scene.append(
        scene.root(),
        NodeKind::Rect(Color(240, 80, 30, 120)),
        fixed(90.5, 4.75),
    );
    scene.set_transform(foreground, Transform { x: 15.25, y: 40.5 });
    let initial = scene.flush().damage;
    gpu.render(&scene, &initial).unwrap();
    let mut seed = 0x921a_51c3u32;
    for frame in 0..64 {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let coordinate = |shift: u32| ((seed >> shift) & 255) as f32 / 4. - 10.25;
        match frame % 8 {
            0 => scene.set_kind(
                backdrop,
                NodeKind::Rect(Color(
                    seed as u8,
                    (seed >> 8) as u8,
                    (seed >> 16) as u8,
                    220,
                )),
            ),
            1 => scene.set_transform(
                backdrop,
                Transform {
                    x: coordinate(0),
                    y: coordinate(8),
                },
            ),
            2 => {
                let mut effects = scene.effects(glass);
                effects.blur_radius = [0., 0.05, 1.5, 6., 20.][(seed as usize) % 5];
                scene.set_effects(glass, effects);
            }
            3 => {
                let mut effects = scene.effects(group);
                effects.opacity = if (frame / 8) % 2 == 0 { 0. } else { 0.65 };
                scene.set_effects(group, effects);
            }
            4 => scene.set_style(
                group,
                Style {
                    clip: (frame / 8) % 2 == 0,
                    ..fixed(
                        62.5 + coordinate(0).abs() / 4.,
                        53.25 + coordinate(8).abs() / 4.,
                    )
                },
            ),
            5 => scene.set_transform(
                glass,
                Transform {
                    x: coordinate(0),
                    y: coordinate(8),
                },
            ),
            6 => {
                if let Some(node) = other.take() {
                    scene.remove(node);
                } else {
                    let node = scene.append(
                        scene.root(),
                        NodeKind::Rect(Color(20, 180, 150, 65)),
                        fixed(34.25, 29.5),
                    );
                    scene.set_transform(node, Transform { x: 54.25, y: 35.5 });
                    scene.set_effects(
                        node,
                        Effects {
                            blur_radius: 3.25,
                            ..Effects::default()
                        },
                    );
                    other = Some(node);
                }
            }
            _ => scene.set_kind(
                foreground,
                NodeKind::Rect(Color((seed >> 16) as u8, 120, 80, 90)),
            ),
        }
        let damage = scene.flush().damage;
        gpu.render(&scene, &damage).unwrap();
        let partial = gpu.readback().unwrap();
        let mut fresh = GpuRenderer::new_with_context(160, 120, &gpu.context()).unwrap();
        fresh.set_scale_factor(1.25);
        fresh.render(&scene, &[scene.bounds(scene.root())]).unwrap();
        assert!(
            partial == fresh.readback().unwrap(),
            "incremental blur differs on deterministic mutation frame {frame}"
        );
    }
}

#[test]
fn bounded_blur_damage_cached_layer_root_samples_changed_parent_backdrop() {
    let _fixture = gpu_fixture();
    let mut gpu = GpuRenderer::new(260, 156).unwrap();
    gpu.set_scale_factor(1.3);
    let mut scene = Scene::new(200., 120.);
    scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
    scene.append(
        scene.root(),
        NodeKind::Rect(Color(15, 30, 45, 255)),
        fixed(200., 120.),
    );
    let backdrop = scene.append(
        scene.root(),
        NodeKind::Rect(Color(245, 220, 80, 255)),
        fixed(12., 26.),
    );
    scene.set_transform(backdrop, Transform { x: 35.25, y: 41.5 });
    let group = scene.append(
        scene.root(),
        NodeKind::Container(Layout::Overlay),
        fixed(60., 45.),
    );
    scene.set_transform(
        group,
        Transform {
            x: 42.25,
            y: 30.125,
        },
    );
    scene.set_isolated(group, true);
    scene.set_effects(
        group,
        Effects {
            opacity: 0.8,
            blur_radius: 4.25,
            ..Effects::default()
        },
    );
    scene.append(
        group,
        NodeKind::Rect(Color(50, 180, 230, 65)),
        fixed(60., 45.),
    );
    let initial = scene.flush().damage;
    assert_eq!(gpu.render(&scene, &initial).unwrap().layer_repaints, 1);
    scene.set_kind(backdrop, NodeKind::Rect(Color(230, 25, 90, 255)));
    let damage = scene.flush().damage;
    let stats = gpu.render(&scene, &damage).unwrap();
    assert_eq!(stats.layer_repaints, 0);
    assert_eq!(stats.layer_cache_hits, 1);
    assert!(stats.damaged_pixels < 260 * 156);
    assert_blur_matches_fresh(&gpu, &scene, 260, 156, 1.3);
    scene.set_transform(
        group,
        Transform {
            x: 45.625,
            y: 32.375,
        },
    );
    let damage = scene.flush().damage;
    let stats = gpu.render(&scene, &damage).unwrap();
    assert_eq!(stats.layer_repaints, 0);
    assert_eq!(stats.layer_cache_hits, 1);
    assert_eq!(stats.layer_texture_allocations, 0);
    assert_blur_matches_fresh(&gpu, &scene, 260, 156, 1.3);
}

#[test]
fn variable_virtual_rows_height_anchor_focus_and_resize_match_full_repaint() {
    let _gpu_fixture = gpu_fixture();
    use std::{cell::Cell, rc::Rc};
    use zgx::{
        compose::prelude::*,
        input::{InputEvent, Key, Modifiers},
        semantics::Role,
        widgets::Ui,
    };
    let mut gpu = GpuRenderer::new(192, 160).unwrap();
    let mut ui = Ui::new(192., 160.);
    let heights = VariableHeights::new(&ui.runtime, 100_000, 16.);
    let mut expected: Vec<f32> = (0..100_000).map(|i| [16., 24., 32., 40.][i % 4]).collect();
    ui.runtime.batch(|| {
        for (index, height) in expected.iter().copied().enumerate() {
            heights.set_height(index, height).unwrap();
        }
    });
    let offset = ui.signal(0.);
    let builds = Rc::new(Cell::new(0));
    let built = builds.clone();
    let mounted = ui.mount(
        variable_virtual_list(
            offset.clone(),
            heights.clone(),
            2,
            |i| i,
            move |_, i, _| {
                built.set(built.get() + 1);
                div().w_full().h_full().bg(if i.is_multiple_of(2) {
                    rgb(0xee3322)
                } else {
                    rgb(0x2244ee)
                })
            },
        )
        .w_full()
        .h_full()
        .p(8.)
        .bg(rgb(0x102030))
        .keyboard_navigation(true),
    );
    let key = |ui: &mut Ui, key| {
        ui.dispatch(InputEvent::KeyDown {
            key,
            modifiers: Modifiers::default(),
            repeat: false,
        });
    };
    let render = |gpu: &mut GpuRenderer,
                  ui: &Ui,
                  expected: &[f32],
                  width: usize,
                  height: usize,
                  stage: &str| {
        ui.prepare_frame();
        let focused = ui.input.focused().filter(|node| {
            ui.semantics
                .borrow()
                .get(*node)
                .is_some_and(|s| s.role == Role::ListItem)
        });
        let focus_bounds = focused.map(|node| ui.scene.borrow().bounds(node));
        let report = ui.scene.borrow_mut().flush();
        gpu.render(&ui.scene.borrow(), &report.damage).unwrap();
        let partial = gpu.readback().unwrap();
        gpu.render(
            &ui.scene.borrow(),
            &[Rect::new(0., 0., width as f32, height as f32)],
        )
        .unwrap();
        assert!(
            partial == gpu.readback().unwrap(),
            "incremental/full mismatch: {stage}"
        );
        let mut prefixes = Vec::with_capacity(expected.len() + 1);
        prefixes.push(0.);
        for h in expected {
            prefixes.push(prefixes.last().unwrap() + h);
        }
        let position = offset.get();
        let pixel = |x: usize, y: usize| &partial[(y * width + x) * 4..(y * width + x) * 4 + 4];
        for y in 0..height {
            if y < 8 || y >= height - 8 {
                // Removing a focused row returns focus to the list, whose
                // default focus variant paints a one-pixel outer border.
                let padding =
                    if ui.input.focused() == Some(mounted.node()) && (y == 0 || y == height - 1) {
                        [94, 165, 255, 255]
                    } else {
                        [16, 32, 48, 255]
                    };
                assert_eq!(
                    pixel(9, y),
                    &padding,
                    "marker escaped padding: {stage} y={y}"
                );
                assert_eq!(
                    pixel(width / 2, y),
                    &padding,
                    "row escaped padding: {stage} y={y}"
                );
                continue;
            }
            let row = prefixes.partition_point(|top| *top <= position + y as f32 - 8. + 0.5) - 1;
            let fill = if row.is_multiple_of(2) {
                [238, 51, 34, 255]
            } else {
                [34, 68, 238, 255]
            };
            assert_eq!(
                pixel(width / 2, y),
                &fill,
                "row boundary mismatch: {stage} y={y}"
            );
            let marker =
                focus_bounds.is_some_and(|b| y as f32 >= b.y && (y as f32) < b.y + b.height);
            let expected_marker = if marker { [94, 165, 255, 255] } else { fill };
            assert_eq!(
                pixel(9, y),
                &expected_marker,
                "focus marker bounds: {stage} y={y}"
            );
        }
        assert!(
            ui.semantics
                .borrow()
                .iter()
                .filter(|(_, s)| s.role == Role::ListItem)
                .count()
                <= 20
        );
        assert!(ui.scene.borrow().paint_items().count() < 70);
        (partial, report.layout_nodes)
    };
    render(&mut gpu, &ui, &expected, 192, 160, "initial");
    assert!(ui.input.focus(&ui.scene, Some(mounted.node())));
    key(&mut ui, Key::Home);
    render(&mut gpu, &ui, &expected, 192, 160, "focused first row");
    let retained_focus = ui.input.focused();
    expected[0] = 36.;
    heights.set_height(0, expected[0]).unwrap();
    render(&mut gpu, &ui, &expected, 192, 160, "focused marker grows");
    assert_eq!(ui.input.focused(), retained_focus);
    expected[0] = 12.;
    heights.set_height(0, expected[0]).unwrap();
    render(
        &mut gpu,
        &ui,
        &expected,
        192,
        160,
        "focused marker shrinks and clears old pixels",
    );

    let anchor_top: f32 = expected[..1000].iter().sum();
    offset.set(anchor_top);
    render(
        &mut gpu,
        &ui,
        &expected,
        192,
        160,
        "large jump disposes former focus",
    );
    key(&mut ui, Key::ArrowDown);
    let anchor_node = ui.input.focused().unwrap();
    assert_eq!(
        ui.semantics
            .borrow()
            .get(anchor_node)
            .unwrap()
            .position_in_set,
        Some(1001)
    );
    offset.set(anchor_top + 3.);
    render(
        &mut gpu,
        &ui,
        &expected,
        192,
        160,
        "partially clipped focused anchor",
    );
    let before_growth = builds.get();
    let anchor_y = ui.scene.borrow().bounds(anchor_node).y;
    expected[0] += 12.;
    heights.set_height(0, expected[0]).unwrap();
    render(&mut gpu, &ui, &expected, 192, 160, "offscreen prefix grows");
    assert_eq!(offset.get(), anchor_top + 15.);
    assert_eq!(ui.scene.borrow().bounds(anchor_node).y, anchor_y);
    assert_eq!(
        builds.get(),
        before_growth,
        "prefix update preserves mounted rows"
    );
    expected[1000] += 20.;
    heights.set_height(1000, expected[1000]).unwrap();
    render(
        &mut gpu,
        &ui,
        &expected,
        192,
        160,
        "visible anchor and marker grow",
    );
    assert_eq!(ui.input.focused(), Some(anchor_node));
    assert_eq!(ui.scene.borrow().bounds(anchor_node).y, anchor_y);
    let before_scroll = builds.get();
    offset.set(offset.get() + 1.);
    let (_, layout_nodes) = render(
        &mut gpu,
        &ui,
        &expected,
        192,
        160,
        "within-range translation",
    );
    assert_eq!(
        layout_nodes, 0,
        "same-range scrolling should remain compositional"
    );
    assert_eq!(builds.get(), before_scroll);

    ui.scene.borrow_mut().resize(224., 192.);
    gpu.resize(224, 192);
    render(&mut gpu, &ui, &expected, 224, 192, "larger viewport");
    ui.scene.borrow_mut().resize(160., 128.);
    gpu.resize(160, 128);
    render(&mut gpu, &ui, &expected, 160, 128, "smaller viewport");
    key(&mut ui, Key::End);
    render(
        &mut gpu,
        &ui,
        &expected,
        160,
        128,
        "last row and clamped extent",
    );
    assert_eq!(
        ui.semantics
            .borrow()
            .get(ui.input.focused().unwrap())
            .unwrap()
            .position_in_set,
        Some(100_000)
    );
    assert!(
        builds.get() < 60,
        "work must stay proportional to mounted ranges"
    );
}

#[test]
fn naturally_measured_streaming_rows_reflow_with_damage_and_stable_anchor() {
    let _gpu_fixture = gpu_fixture();
    use std::{cell::Cell, rc::Rc};
    use zgx::{
        compose::prelude::*,
        semantics::Role,
        text_layout::{FontStyle, TextLayout},
        widgets::Ui,
    };
    let mut gpu = GpuRenderer::new(240, 180).unwrap();
    let mut ui = Ui::new(240., 180.);
    let fonts = gpu.text_system();
    ui.scene.borrow_mut().set_font_text_shaper(
        move |text: &str, size, width, font: &FontStyle| -> Box<dyn TextLayout> {
            Box::new(zgx_gpu::text::ShapedText::with_font(
                &mut fonts.borrow_mut(),
                text,
                size,
                width,
                font,
            ))
        },
    );
    let heights = VariableHeights::new(&ui.runtime, 100_000, 48.);
    let offset = ui.signal(0.);
    let message = ui.signal("short message".to_owned());
    let earlier = ui.signal("earlier message".to_owned());
    let read = message.clone();
    let read_earlier = earlier.clone();
    let builds = Rc::new(Cell::new(0));
    let created = builds.clone();
    let mounted = ui.mount(
        measured_virtual_list(
            offset.clone(),
            heights.clone(),
            1,
            |i| i,
            move |_, i, _| {
                created.set(created.get() + 1);
                let value = if i == 0 {
                    read_earlier.clone()
                } else {
                    read.clone()
                };
                column()
                    .id(format!("row-{i}"))
                    .w_full()
                    .p(4.)
                    .gap(2.)
                    .bg(rgb(if i % 2 == 0 { 0x203040 } else { 0x304050 }))
                    .child(text(format!("Row {i}")).h(16.))
                    .child(
                        text_signal(move || {
                            if i <= 1 {
                                value.get()
                            } else {
                                "stable row".to_owned()
                            }
                        })
                        .w_full()
                        .text_wrap(true),
                    )
            },
        )
        .w_full()
        .h_full()
        .p(4.)
        .text_size(14.)
        .line_height(18.)
        .text_color(rgb(0xeeeeee))
        .keyboard_navigation(true),
    );
    let render = |gpu: &mut GpuRenderer, ui: &Ui, width: u32, height: u32| {
        ui.try_prepare_frame().unwrap();
        let report = ui.scene.borrow_mut().flush();
        gpu.render(&ui.scene.borrow(), &report.damage).unwrap();
        let partial = gpu.readback().unwrap();
        gpu.render(
            &ui.scene.borrow(),
            &[Rect::new(0., 0., width as f32, height as f32)],
        )
        .unwrap();
        assert!(
            partial == gpu.readback().unwrap(),
            "natural reflow must preserve damage coverage"
        );
        let mut rows: Vec<_> = ui
            .semantics
            .borrow()
            .iter()
            .filter(|(_, semantic)| semantic.role == Role::ListItem)
            .map(|(node, semantic)| (semantic.position_in_set.unwrap(), node))
            .collect();
        rows.sort_by_key(|(position, _)| *position);
        let scene = ui.scene.borrow();
        for pair in rows.windows(2) {
            let first = scene.bounds(pair[0].1);
            let second = scene.bounds(pair[1].1);
            assert!(
                (first.y + first.height - second.y).abs() < 0.01,
                "measured visible rows must meet without overlap or gaps"
            );
        }
        assert!(rows.len() <= 10);
        partial
    };
    render(&mut gpu, &ui, 240, 180);
    offset.set(heights.row_offset(1) + 5.);
    render(&mut gpu, &ui, 240, 180);
    let row_one = mounted.find("row-1").unwrap();
    let wrapper = ui.scene.borrow().parent(row_one).unwrap();
    ui.input.focus(&ui.scene, Some(wrapper));
    let anchor = ui.scene.borrow().bounds(wrapper).y;
    let original = heights.row_height(1).unwrap();
    for text in [
        "streaming words added to this retained message ".repeat(3),
        "more text with multiple wrapped lines ".repeat(6),
    ] {
        message.set(text);
        render(&mut gpu, &ui, 240, 180);
        assert_eq!(mounted.find("row-1"), Some(row_one));
        assert_eq!(ui.scene.borrow().bounds(wrapper).y, anchor);
        assert_eq!(ui.input.focused(), Some(wrapper));
    }
    assert!(heights.row_height(1).unwrap() > original);
    earlier.set("an earlier visible overscan message grows above the anchor ".repeat(4));
    render(&mut gpu, &ui, 240, 180);
    assert_eq!(ui.scene.borrow().bounds(wrapper).y, anchor);
    let wide = heights.row_height(1).unwrap();
    ui.scene.borrow_mut().resize(160., 180.);
    gpu.resize(160, 180);
    render(&mut gpu, &ui, 160, 180);
    assert!(heights.row_height(1).unwrap() > wide);
    assert_eq!(ui.scene.borrow().bounds(wrapper).y, anchor);
    let count = builds.get();
    offset.set(offset.get() + 1.);
    render(&mut gpu, &ui, 160, 180);
    assert_eq!(builds.get(), count);
    offset.set(heights.row_offset(5000));
    render(&mut gpu, &ui, 160, 180);
    assert!(mounted.find("row-1").is_none());
    assert!(builds.get() < 40);
}

#[test]
fn clean_blur_streaming_uses_one_pass_including_isolated_layer_accounting() {
    let _fixture = gpu_fixture();
    for isolated in [false, true] {
        let mut gpu = GpuRenderer::new(240, 160).unwrap();
        let mut scene = Scene::new(240., 160.);
        scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
        let group = scene.append(
            scene.root(),
            NodeKind::Container(Layout::Overlay),
            fixed(240., 160.),
        );
        scene.set_isolated(group, isolated);
        scene.append(
            group,
            NodeKind::Rect(Color(20, 40, 60, 255)),
            fixed(240., 160.),
        );
        let blur = scene.append(
            group,
            NodeKind::Rect(Color(230, 40, 70, 35)),
            fixed(32., 32.),
        );
        scene.set_transform(blur, Transform { x: 16., y: 16. });
        scene.set_effects(
            blur,
            Effects {
                blur_radius: 2.,
                ..Effects::default()
            },
        );
        let text_kind = |n| NodeKind::Text {
            text: format!("Stream {n}").into(),
            color: Color(255, 255, 255, 255),
            font_size: 14.,
        };
        let text = scene.append(group, text_kind(0), fixed(100., 24.));
        scene.set_transform(text, Transform { x: 130., y: 120. });
        let damage = scene.flush().damage;
        let initial = gpu.render(&scene, &damage).unwrap();
        assert_eq!(initial.blur_passes, 3);
        assert!(initial.render_passes > initial.blur_passes);
        for n in 1..5 {
            scene.set_kind(text, text_kind(n));
            let damage = scene.flush().damage;
            let stats = gpu.render(&scene, &damage).unwrap();
            if isolated {
                // The layer repaints where the text changed alone: the blur,
                // away from it, draws nothing again (`layer_partial_repaints`).
                assert_eq!((stats.layer_repaints, stats.layer_partial_repaints), (1, 1));
                assert_eq!(stats.blur_passes, 0);
                assert_eq!(stats.render_passes, 2, "the layer's and the window's");
            } else {
                assert_eq!(stats.blur_passes, 0);
                assert_eq!(stats.render_passes, 1);
            }
            assert_blur_matches_fresh(&gpu, &scene, 240, 160, 1.);
        }
        let idle = gpu.render(&scene, &[]).unwrap();
        assert_eq!((idle.render_passes, idle.blur_passes), (0, 0));
    }
}

#[test]
fn retained_rich_text_colors_wrap_and_reactive_damage_match_full_repaint() {
    let _fixture = gpu_fixture();
    use std::sync::Arc;
    use zgx::rich_text::{RichText, TextRun};
    let mut gpu = GpuRenderer::new(220, 130).unwrap();
    let mut scene = Scene::new(220., 130.);
    let make = |tail: &str, color: Color| {
        let content = format!("RED {tail}");
        Arc::new(
            RichText::new(
                content.as_str(),
                vec![
                    TextRun {
                        range: 0..4,
                        font: Default::default(),
                        font_size: 22.,
                        color: Color(255, 30, 30, 255),
                        ..Default::default()
                    },
                    TextRun {
                        range: 4..content.len(),
                        font: zgx::text_layout::FontStyle {
                            weight: 700,
                            ..Default::default()
                        },
                        font_size: 27.,
                        color,
                        ..Default::default()
                    },
                ],
            )
            .unwrap(),
        )
    };
    let text = scene.append(
        scene.root(),
        NodeKind::RichText {
            text: make("GREEN words wrap", Color(30, 255, 30, 255)),
        },
        fixed(130., 120.),
    );
    let initial = scene.flush();
    let stats = gpu.render(&scene, &initial.damage).unwrap();
    assert_eq!(stats.shaped_nodes, 1);
    let pixels = gpu.readback().unwrap();
    assert!(
        pixels
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[0] > p[1].saturating_add(60))
            .count()
            > 20
    );
    assert!(
        pixels
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[1] > p[0].saturating_add(60))
            .count()
            > 20
    );
    assert_eq!(
        gpu.render(&scene, &[Rect::new(0., 0., 220., 130.)])
            .unwrap()
            .shaped_nodes,
        0
    );
    for value in ["BLUE", "BLUE more words streaming", "العربية end"] {
        scene.set_kind(
            text,
            NodeKind::RichText {
                text: make(value, Color(30, 30, 255, 255)),
            },
        );
        let frame = scene.flush();
        gpu.render(&scene, &frame.damage).unwrap();
        assert_blur_matches_fresh(&gpu, &scene, 220, 130, 1.);
    }
    scene.set_kind(
        text,
        NodeKind::RichText {
            text: Arc::new(RichText::new("", Vec::new()).unwrap()),
        },
    );
    let frame = scene.flush();
    gpu.render(&scene, &frame.damage).unwrap();
    assert_blur_matches_fresh(&gpu, &scene, 220, 130, 1.);
}

#[test]
fn rich_text_decorations_follow_wrapped_bidi_fragments_and_invalidate() {
    let _fixture = gpu_fixture();
    use std::sync::Arc;
    use zgx::rich_text::{Decoration, RichText, TextRun};
    let mut scene = Scene::new(220., 160.);
    let mut gpu = GpuRenderer::new(220, 160).unwrap();
    let value = "Latin العربية wrapped words";
    let mut run = TextRun {
        range: 0..value.len(),
        font_size: 24.,
        background: Some(Color(20, 60, 100, 255)),
        underline: Some(Decoration::new(2.).color(Color(255, 255, 0, 255))),
        strikethrough: Some(Decoration::new(2.).color(Color(255, 0, 255, 255))),
        ..Default::default()
    };
    let node = scene.append(
        scene.root(),
        NodeKind::RichText {
            text: Arc::new(RichText::new(value, vec![run.clone()]).unwrap()),
        },
        fixed(150., 150.),
    );
    let frame = scene.flush();
    gpu.render(&scene, &frame.damage).unwrap();
    let pixels = gpu.readback().unwrap();
    for expected in [[20, 60, 100, 255], [255, 255, 0, 255], [255, 0, 255, 255]] {
        assert!(
            pixels
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|p| **p == expected)
                .count()
                > 30,
            "missing decoration {expected:?}"
        );
    }
    run.background = None;
    run.underline = None;
    run.strikethrough = None;
    scene.set_kind(
        node,
        NodeKind::RichText {
            text: Arc::new(RichText::new(value, vec![run]).unwrap()),
        },
    );
    let frame = scene.flush();
    assert_eq!(frame.layout_nodes, 0);
    gpu.render(&scene, &frame.damage).unwrap();
    assert_blur_matches_fresh(&gpu, &scene, 220, 160, 1.);
}

#[test]
fn rich_text_display_overflow_reuses_shapes_and_restores_removed_lines() {
    let _fixture = gpu_fixture();
    use zgx::text_layout::{TextOptions, TextOverflow};
    let mut scene = Scene::new(240., 160.);
    let mut gpu = GpuRenderer::new(240, 160).unwrap();
    let options = TextOptions {
        overflow: TextOverflow::Ellipsis,
        line_clamp: std::num::NonZeroU32::new(1),
    };
    let node = scene.append(
        scene.root(),
        NodeKind::Text {
            text: "one two three four five six seven eight nine ten".into(),
            font_size: 22.,
            color: Color(245, 245, 245, 255),
        },
        Style {
            text_options: options,
            ..fixed(130., 150.)
        },
    );
    let frame = scene.flush();
    let stats = gpu.render(&scene, &frame.damage).unwrap();
    assert_eq!(stats.shaped_nodes, 1);
    let truncated = gpu.readback().unwrap();
    assert!(
        truncated
            .as_chunks::<4>()
            .0
            .iter()
            .enumerate()
            .all(|(i, p)| i / 240 < 35 || p[0] < 100)
    );
    assert_eq!(
        gpu.render(&scene, &[Rect::new(0., 0., 240., 160.)])
            .unwrap()
            .shaped_nodes,
        0
    );
    let mut style = scene.style(node);
    style.text_options = Default::default();
    scene.set_style(node, style);
    let frame = scene.flush();
    gpu.render(&scene, &frame.damage).unwrap();
    assert!(
        gpu.readback()
            .unwrap()
            .as_chunks::<4>()
            .0
            .iter()
            .enumerate()
            .any(|(i, p)| i / 240 > 40 && p[0] > 150)
    );
    assert_blur_matches_fresh(&gpu, &scene, 240, 160, 1.);
    let mut style = scene.style(node);
    style.text_options = options;
    scene.set_style(node, style);
    let frame = scene.flush();
    gpu.render(&scene, &frame.damage).unwrap();
    assert_eq!(gpu.readback().unwrap(), truncated);
}

#[test]
fn recycled_glyph_atlas_does_not_bleed_stale_texels_into_fractional_glyphs() {
    let _gpu_fixture = gpu_fixture();
    fn probe(scene: &mut Scene, text: &str, size: f32, padding: f32) {
        scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
        scene.set_style(
            scene.root(),
            Style {
                padding,
                ..fixed(160., 160.)
            },
        );
        let old: Vec<_> = scene.children(scene.root()).to_vec();
        for child in old {
            scene.remove(child);
        }
        scene.append(
            scene.root(),
            NodeKind::Text {
                text: text.into(),
                font_size: size,
                color: Color(255, 255, 255, 255),
            },
            fixed(150., 150.),
        );
    }
    let render = |gpu: &mut GpuRenderer, scene: &mut Scene| {
        scene.flush();
        gpu.render(scene, &[Rect::new(0., 0., 160., 160.)]).unwrap();
    };
    // Fill the atlas with opaque blocks of distinct sizes until it recycles.
    let mut gpu = GpuRenderer::new(160, 160).unwrap();
    let mut scene = Scene::new(160., 160.);
    let mut previous = 0;
    let mut recycled = false;
    for size in 60..200 {
        probe(&mut scene, "█", size as f32, 0.);
        render(&mut gpu, &mut scene);
        let entries = gpu.debug_cache_stats().atlas_entries;
        if entries < previous {
            recycled = true;
            break;
        }
        previous = entries;
    }
    assert!(recycled, "atlas never recycled");
    assert_eq!(gpu.debug_cache_stats().atlas_bytes, 512 * 512 * 4);
    // A thin glyph at a fractional offset samples its atlas neighbours.
    probe(&mut scene, "o", 48., 10.37);
    render(&mut gpu, &mut scene);
    let recycled = gpu.readback().unwrap();
    let mut fresh_gpu = GpuRenderer::new(160, 160).unwrap();
    let mut fresh_scene = Scene::new(160., 160.);
    probe(&mut fresh_scene, "o", 48., 10.37);
    render(&mut fresh_gpu, &mut fresh_scene);
    let fresh = fresh_gpu.readback().unwrap();
    let worst = recycled
        .iter()
        .zip(&fresh)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap();
    assert!(
        worst <= 2,
        "stale atlas texels bled into the glyph: {worst}"
    );
}

#[test]
fn damaged_blur_batches_surrounding_draws_instead_of_one_pass_each() {
    let _gpu_fixture = gpu_fixture();
    let mut gpu = GpuRenderer::new(200, 200).unwrap();
    let mut s = Scene::new(200., 200.);
    s.set_kind(s.root(), NodeKind::Container(Layout::Overlay));
    // Many distinct draws behind and in front of one filter.
    for i in 0..60 {
        let node = s.append(
            s.root(),
            NodeKind::Rect(Color((i * 4) as u8, 80, 160, 255)),
            fixed(20., 20.),
        );
        s.set_transform(
            node,
            Transform {
                x: (i % 10) as f32 * 20.,
                y: (i / 10) as f32 * 30.,
            },
        );
    }
    let glass = s.append(
        s.root(),
        NodeKind::Rect(Color(255, 0, 0, 40)),
        fixed(200., 200.),
    );
    for i in 0..60 {
        let node = s.append(
            s.root(),
            NodeKind::Rect(Color(40, (i * 4) as u8, 40, 255)),
            fixed(10., 10.),
        );
        s.set_transform(
            node,
            Transform {
                x: (i % 10) as f32 * 20. + 5.,
                y: (i / 10) as f32 * 30. + 5.,
            },
        );
    }
    let d = s.flush().damage;
    gpu.render(&s, &d).unwrap();
    // An animating filter: its radius changes every frame.
    for radius in [2., 4., 6.] {
        s.set_effects(
            glass,
            Effects {
                blur_radius: radius,
                ..Default::default()
            },
        );
        let d = s.flush().damage;
        let stats = gpu.render(&s, &d).unwrap();
        assert_eq!(stats.blur_passes, 3);
        // Clear+draws, the three blur passes, then the blur drawn back and the rest.
        assert!(
            stats.render_passes <= 5,
            "{} render passes for one filter",
            stats.render_passes
        );
    }
}

#[test]
fn many_separate_damage_regions_share_one_render_pass() {
    let _gpu_fixture = gpu_fixture();
    let mut gpu = GpuRenderer::new(400, 400).unwrap();
    let mut s = Scene::new(400., 400.);
    s.set_kind(s.root(), NodeKind::Container(Layout::Overlay));
    // Scattered small animations, like loader dots and shimmer letters.
    let dots: Vec<_> = (0..24)
        .map(|i| {
            let dot = s.append(
                s.root(),
                NodeKind::Rect(Color(200, 200, 255, 255)),
                fixed(8., 8.),
            );
            s.set_transform(
                dot,
                Transform {
                    x: (i % 6) as f32 * 64.,
                    y: (i / 6) as f32 * 96.,
                },
            );
            dot
        })
        .collect();
    let d = s.flush().damage;
    gpu.render(&s, &d).unwrap();
    for frame in 1..4 {
        for dot in &dots {
            s.set_effects(
                *dot,
                Effects {
                    opacity: 0.25 * frame as f32,
                    ..Default::default()
                },
            );
        }
        let d = s.flush().damage;
        assert!(d.len() > 1, "expected separate damage regions");
        let stats = gpu.render(&s, &d).unwrap();
        assert_eq!(stats.render_passes, 1);
    }
}
#[test]
fn resized_layers_reuse_their_texture_and_compose_like_a_fresh_one() {
    let _gpu_fixture = gpu_fixture();
    // A blurred, faded layer that shrinks every frame at a fractional offset:
    // its texture keeps the larger frame's pixels past the new content.
    let build = |width: f32| {
        let mut scene = Scene::new(160., 120.);
        scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
        scene.append(
            scene.root(),
            NodeKind::Rect(Color(10, 12, 16, 255)),
            fixed(160., 120.),
        );
        let group = scene.append(
            scene.root(),
            NodeKind::Container(Layout::Overlay),
            fixed(width, 60.),
        );
        scene.set_isolated(group, true);
        scene.set_transform(group, Transform { x: 20.3, y: 30.7 });
        scene.set_effects(
            group,
            Effects {
                opacity: 0.8,
                edge_fade: 4.,
                ..Default::default()
            },
        );
        let inner = scene.append(
            group,
            NodeKind::Rect(Color(240, 200, 40, 90)),
            fixed(width, 60.),
        );
        scene.set_effects(
            inner,
            Effects {
                blur_radius: 3.,
                ..Default::default()
            },
        );
        scene.append(
            group,
            NodeKind::Rect(Color(40, 160, 255, 255)),
            fixed(width - 10., 50.),
        );
        (scene, group)
    };
    let mut animated = GpuRenderer::new(320, 240).unwrap();
    animated.set_scale_factor(2.);
    let (mut scene, group) = build(120.);
    let damage = scene.flush().damage;
    animated.render(&scene, &damage).unwrap();
    let mut allocations = 0;
    for step in 1..=10 {
        let width = 120. - step as f32 * 7.3;
        scene.set_style(group, fixed(width, 60.));
        let [blurred, top] = scene.children(group) else {
            unreachable!()
        };
        let (blurred, top) = (*blurred, *top);
        scene.set_style(blurred, fixed(width, 60.));
        scene.set_style(top, fixed(width - 10., 50.));
        let damage = scene.flush().damage;
        allocations += animated
            .render(&scene, &damage)
            .unwrap()
            .layer_texture_allocations;
    }
    assert_eq!(allocations, 0, "a shrinking layer keeps its texture");
    let mut fresh = GpuRenderer::new(320, 240).unwrap();
    fresh.set_scale_factor(2.);
    let (mut expected, _) = build(120. - 73.);
    let damage = expected.flush().damage;
    fresh.render(&expected, &damage).unwrap();
    assert!(
        animated.readback().unwrap() == fresh.readback().unwrap(),
        "slack texels leaked into the composed layer"
    );
}
#[test]
fn colour_only_rich_text_changes_recolour_without_reshaping() {
    use zgx::rich_text::{Decoration, RichText, TextRun};
    let _gpu_fixture = gpu_fixture();
    // A fading word with a highlighted code span and an underline: every
    // colour changes each step, the text and runs never do.
    let make = |alpha: u8| {
        let mut underline = Decoration::new(1.5);
        underline.color = Some(Color(90, 200, 255, alpha));
        std::sync::Arc::new(
            RichText::new(
                "Found it: invalidate_all() clears every row",
                vec![
                    TextRun {
                        range: 0..10,
                        font_size: 16.,
                        color: Color(255, 255, 255, 230),
                        ..Default::default()
                    },
                    TextRun {
                        range: 10..26,
                        font_size: 15.,
                        color: Color(124, 134, 255, alpha),
                        background: Some(Color(124, 134, 255, alpha / 8)),
                        ..Default::default()
                    },
                    TextRun {
                        range: 26..43,
                        font_size: 16.,
                        color: Color(255, 255, 255, alpha),
                        underline: Some(underline),
                        ..Default::default()
                    },
                ],
            )
            .unwrap(),
        )
    };
    let build = |alpha: u8| {
        let mut scene = Scene::new(200., 90.);
        scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
        scene.append(
            scene.root(),
            NodeKind::Rect(Color(20, 24, 32, 255)),
            fixed(200., 90.),
        );
        let text = scene.append(
            scene.root(),
            NodeKind::RichText { text: make(alpha) },
            fixed(190., 80.),
        );
        (scene, text)
    };
    let mut gpu = GpuRenderer::new(400, 180).unwrap();
    gpu.set_scale_factor(2.);
    let (mut scene, text) = build(40);
    let damage = scene.flush().damage;
    assert_eq!(gpu.render(&scene, &damage).unwrap().shaped_nodes, 1);
    for alpha in [90, 160, 255] {
        scene.set_kind(text, NodeKind::RichText { text: make(alpha) });
        let damage = scene.flush().damage;
        let stats = gpu.render(&scene, &damage).unwrap();
        assert_eq!(stats.shaped_nodes, 0, "alpha {alpha} re-shaped");
        let mut fresh = GpuRenderer::new(400, 180).unwrap();
        fresh.set_scale_factor(2.);
        let (mut expected, _) = build(alpha);
        let damage = expected.flush().damage;
        fresh.render(&expected, &damage).unwrap();
        assert!(
            gpu.readback().unwrap() == fresh.readback().unwrap(),
            "alpha {alpha}: recoloured pixels differ from a fresh shape"
        );
    }
    // A text change still re-shapes.
    let mut changed = (*make(255)).clone();
    changed = RichText::new("Found it: invalidate_all() clears every rows", {
        let mut runs = changed.runs().to_vec();
        runs[2].range = 26..44;
        runs
    })
    .unwrap();
    scene.set_kind(
        text,
        NodeKind::RichText {
            text: std::sync::Arc::new(changed),
        },
    );
    let damage = scene.flush().damage;
    assert_eq!(gpu.render(&scene, &damage).unwrap().shaped_nodes, 1);
}
#[test]
fn fade_edges_mask_descendants_with_a_quadratic_ramp() {
    let _gpu_fixture = gpu_fixture();
    let mut scene = Scene::new(100., 100.);
    scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
    scene.append(
        scene.root(),
        NodeKind::Rect(Color(0, 0, 0, 255)),
        fixed(100., 100.),
    );
    let viewport = scene.append(
        scene.root(),
        NodeKind::Container(Layout::Overlay),
        Style {
            clip: true,
            fade_edges: [20., 20.],
            ..fixed(100., 100.)
        },
    );
    scene.append(
        viewport,
        NodeKind::Rect(Color(255, 255, 255, 255)),
        fixed(100., 100.),
    );
    let mut gpu = GpuRenderer::new(100, 100).unwrap();
    let damage = scene.flush().damage;
    gpu.render(&scene, &damage).unwrap();
    let pixels = gpu.readback().unwrap();
    let at = |y: usize| pixels[(y * 100 + 50) * 4];
    // Edges nearly vanish, the ramp is quadratic, the middle is untouched.
    assert!(at(0) < 5, "top edge {}", at(0));
    assert!(at(99) < 5, "bottom edge {}", at(99));
    let half = at(10) as f32 / 255.;
    assert!((half - 0.27).abs() < 0.08, "halfway {half}");
    assert_eq!(at(50), 255);
    assert_eq!(at(30), 255);
}

#[test]
fn fade_insets_end_the_bands_inside_the_clip() {
    // A scroll view running on under a bar: its bottom band ends 30 px above
    // the clip's bottom, hiding what lies below.
    let _gpu_fixture = gpu_fixture();
    let mut scene = Scene::new(100., 100.);
    scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
    scene.append(
        scene.root(),
        NodeKind::Rect(Color(0, 0, 0, 255)),
        fixed(100., 100.),
    );
    let viewport = scene.append(
        scene.root(),
        NodeKind::Container(Layout::Overlay),
        Style {
            clip: true,
            fade_edges: [0., 20.],
            fade_insets: [0., 30.],
            ..fixed(100., 100.)
        },
    );
    scene.append(
        viewport,
        NodeKind::Rect(Color(255, 255, 255, 255)),
        fixed(100., 100.),
    );
    let mut gpu = GpuRenderer::new(100, 100).unwrap();
    let damage = scene.flush().damage;
    gpu.render(&scene, &damage).unwrap();
    let pixels = gpu.readback().unwrap();
    let at = |y: usize| pixels[(y * 100 + 50) * 4];
    assert_eq!(at(10), 255);
    assert_eq!(at(49), 255);
    let half = at(60) as f32 / 255.;
    assert!((half - 0.27).abs() < 0.08, "halfway {half}");
    assert!(at(69) < 5, "band's end {}", at(69));
    assert_eq!(at(85), 0);
}

/// Whole-pixel scrolls shift the retained pixels and draw only the exposed
/// strip; the result matches a fresh renderer exactly. A list fading at its
/// edges copies its interior and repaints its bands. A sibling painting over
/// the list, or a fractional shift, make a copy inexact, so those repaint
/// instead.
#[test]
fn scroll_copies_match_a_fresh_render_and_decline_when_inexact() {
    let _gpu_fixture = gpu_fixture();
    use zgx::{compose::prelude::*, widgets::Ui};
    #[derive(Clone, Copy, PartialEq, Debug)]
    enum Case {
        Plain,
        Faded,
        Covered,
        // The list clips at rounded corners: its corners' edge and what lies
        // outside them must not move with the rows.
        Rounded,
    }
    for case in [Case::Plain, Case::Faded, Case::Covered, Case::Rounded] {
        let mut gpu = GpuRenderer::new(160, 120).unwrap();
        let mut ui = Ui::new(160., 120.);
        let offset = ui.signal(0_f32);
        let rows = virtual_list(
            offset.clone(),
            24.,
            2,
            || 1000,
            |i| i,
            |_, key, _| {
                div()
                    .size(144., 24.)
                    .bg(if key % 2 == 0 {
                        rgb(0x223344)
                    } else {
                        rgb(0x334455)
                    })
                    .child(
                        text(format!("Row {key}"))
                            .text_size(13.)
                            .text_color(rgb(0xffffff)),
                    )
            },
        )
        .w(160.)
        .h(100.)
        .p(8.)
        .bg(rgb(0x101820));
        let rows = match case {
            Case::Faded => rows.fade_edges(12., 12.),
            Case::Rounded => rows.rounded(14.),
            _ => rows,
        };
        let mut root = overlay().w(160.).h(120.).bg(rgb(0x000000)).child(rows);
        if case == Case::Covered {
            root = root.child(
                div()
                    .absolute()
                    .left(40.)
                    .top(40.)
                    .size(30., 30.)
                    .bg(rgba(0xffffff80)),
            );
        }
        ui.mount(root);
        let mut copies = 0;
        // Down, then back up: what a move left behind shows on the way back.
        for scroll in [0., 14., 28., 100., 101.5, 150., 40., 26., 12., 0.] {
            offset.set(scroll);
            ui.prepare_frame();
            let damage = ui.scene.borrow_mut().flush().damage;
            copies += gpu
                .render(&ui.scene.borrow(), &damage)
                .unwrap()
                .scroll_copies;
            let mut fresh = GpuRenderer::new(160, 120).unwrap();
            fresh
                .render(&ui.scene.borrow(), &[Rect::new(0., 0., 160., 120.)])
                .unwrap();
            assert_eq!(
                gpu.readback().unwrap(),
                fresh.readback().unwrap(),
                "{case:?} scroll {scroll}"
            );
        }
        if cfg!(target_os = "macos") {
            // Texture blits cost Apple GPUs graphics memory; macOS repaints.
            assert_eq!(copies, 0, "{case:?}");
            continue;
        }
        match case {
            // 0→14, 14→28 and 28→100 copy. 100→101.5 and 101.5→150 shift by
            // fractions of a pixel; 150→40 jumps past the viewport.
            Case::Plain => assert!(copies >= 3, "{copies}"),
            // Its interior copies, its bands repaint (bands.rs).
            Case::Faded => assert!(copies >= 3, "{copies}"),
            Case::Rounded => assert!(copies >= 3, "{copies}"),
            // Only 28→100 copies: its 72 px exposed strip repaints the whole
            // overlay, which the smaller moves would have shifted.
            Case::Covered => assert_eq!(copies, 1),
        }
    }
}

/// A sticky header holds still over rows a copy moves: what the copy took
/// of it repaints, and each frame matches a fresh renderer.
#[test]
fn scroll_copies_under_a_sticky_header_match_a_fresh_render() {
    let _gpu_fixture = gpu_fixture();
    use zgx::{compose::prelude::*, widgets::Ui};
    let mut gpu = GpuRenderer::new(160, 120).unwrap();
    let mut ui = Ui::new(160., 120.);
    let offset = ui.signal(0_f32);
    let section = |shade: u32| {
        column()
            .w(160.)
            .h(200.)
            .bg(rgb(shade))
            .child(div().w(160.).h(20.).bg(rgb(0xff8800)).sticky_top(6.))
            .child(div().w(100.).h(170.).bg(rgb(0x223344)))
    };
    ui.mount(
        scroll(offset.clone())
            .w(160.)
            .h(120.)
            .bg(rgb(0x000000))
            .child(section(0x101820))
            .child(section(0x182010)),
    );
    let mut copies = 0;
    // On through the first header's hold and the hand-over to the second.
    for at in [
        0., 3., 6., 14., 28., 100., 160., 174., 188., 200., 214., 180., 90., 0.,
    ] {
        offset.set(at);
        ui.prepare_frame();
        let damage = ui.scene.borrow_mut().flush().damage;
        copies += gpu
            .render(&ui.scene.borrow(), &damage)
            .unwrap()
            .scroll_copies;
        let mut fresh = GpuRenderer::new(160, 120).unwrap();
        fresh
            .render(&ui.scene.borrow(), &[Rect::new(0., 0., 160., 120.)])
            .unwrap();
        assert_eq!(
            gpu.readback().unwrap(),
            fresh.readback().unwrap(),
            "scroll {at}"
        );
    }
    if !cfg!(target_os = "macos") {
        assert!(copies >= 3, "{copies}");
    }
}

/// A trackpad's fractional offsets, with the scene on the device-pixel grid
/// (`Scene::set_pixel_scale`, as a window sets it): the content moves by whole
/// pixels, so each move copies, and the frames match a fresh render.
#[test]
fn fractional_scrolls_on_the_pixel_grid_copy_and_match_a_fresh_render() {
    let _gpu_fixture = gpu_fixture();
    use zgx::{compose::prelude::*, widgets::Ui};
    let mut gpu = GpuRenderer::new(160, 120).unwrap();
    let mut ui = Ui::new(160., 120.);
    ui.scene.borrow_mut().set_pixel_scale(1.);
    let offset = ui.signal(0_f32);
    ui.mount(
        overlay().w(160.).h(120.).bg(rgb(0x000000)).child(
            virtual_list(
                offset.clone(),
                24.,
                2,
                || 1000,
                |i| i,
                |_, key, _| {
                    div()
                        .size(144., 24.)
                        .bg(if key % 2 == 0 {
                            rgb(0x223344)
                        } else {
                            rgb(0x334455)
                        })
                        .child(div().size(144., 1.).bg(rgb(0xffffff)))
                },
            )
            .w(160.)
            .h(100.)
            .p(8.)
            .bg(rgb(0x101820)),
        ),
    );
    let mut copies = 0;
    for scroll in [0., 14.3, 28.6, 31.45, 60.2] {
        offset.set(scroll);
        ui.prepare_frame();
        let damage = ui.scene.borrow_mut().flush().damage;
        copies += gpu
            .render(&ui.scene.borrow(), &damage)
            .unwrap()
            .scroll_copies;
        let mut fresh = GpuRenderer::new(160, 120).unwrap();
        fresh
            .render(&ui.scene.borrow(), &[Rect::new(0., 0., 160., 120.)])
            .unwrap();
        assert_eq!(
            gpu.readback().unwrap(),
            fresh.readback().unwrap(),
            "scroll {scroll}"
        );
    }
    // Texture blits cost Apple GPUs graphics memory; macOS repaints.
    let expected = if cfg!(target_os = "macos") { 0 } else { 4 };
    assert_eq!(copies, expected);
}

/// A rounded panel, overflow hidden, holding a scroll view beside a divider
/// (an app's sidebar), scrolled down and back up in a wheel's uneven steps
/// on a 2x pixel grid: a copy of the view's pixels must not carry the
/// panel's corners, which stay where they are, down the view.
#[test]
fn rounded_panel_scrolled_down_and_up_matches_a_fresh_render() {
    let _gpu_fixture = gpu_fixture();
    use zgx::{compose::prelude::*, widgets::Ui};
    let (w, h) = (160., 120.);
    let renderer = || {
        let mut gpu = GpuRenderer::new(320, 240).unwrap();
        gpu.set_scale_factor(2.);
        gpu
    };
    let mut gpu = renderer();
    let mut ui = Ui::new(w, h);
    ui.scene.borrow_mut().set_pixel_scale(2.);
    let offset = ui.signal(0_f32);
    let mut cards = column().gap(6.).p(6.);
    for i in 0..30 {
        cards = cards.child(div().w(96.).h(28.).bg(if i % 2 == 0 {
            rgb(0x223344)
        } else {
            rgb(0x2a3a4a)
        }));
    }
    ui.mount(
        overlay().w(w).h(h).bg(rgb(0x2f6b3a)).child(
            row()
                .w(150.)
                .h(110.)
                .left(5.)
                .top(5.)
                .rounded(14.)
                .bg(rgb(0x111111))
                .overflow_hidden()
                // Filled opaque and square: its moves copy where copying is
                // allowed, and the panel's corners round it.
                .child(
                    scroll(offset.clone())
                        .w(110.)
                        .h(110.)
                        .bg(rgb(0x111111))
                        .child(cards),
                )
                .child(div().w(1.).h(110.).bg(rgb(0x333333)))
                .child(div().w(38.).h(110.).bg(rgb(0x161616))),
        ),
    );
    let steps = [
        0., 3.5, 9.25, 17.0, 30.5, 48.0, 71.75, 90.0, 120.5, 150.0, 120.0, 95.5, 61.25, 40.0, 22.5,
        8.75, 2.0, 0.,
    ];
    for scroll_to in steps {
        offset.set(scroll_to);
        ui.prepare_frame();
        let damage = ui.scene.borrow_mut().flush().damage;
        gpu.render(&ui.scene.borrow(), &damage).unwrap();
        let mut fresh = renderer();
        fresh
            .render(&ui.scene.borrow(), &[Rect::new(0., 0., w, h)])
            .unwrap();
        assert!(
            gpu.readback().unwrap() == fresh.readback().unwrap(),
            "scroll {scroll_to}: differs from a fresh render"
        );
    }
}

/// A chat transcript: a measured, end-anchored virtual list of wrapped text
/// rows with estimated heights, fading edges and a scrollbar. Scrolling in
/// uneven steps mounts, measures and unmounts rows; every partial frame must
/// match a full repaint.
#[test]
fn measured_transcript_scrolling_matches_full_repaint() {
    let _gpu_fixture = gpu_fixture();
    use zgx::{compose::prelude::*, widgets::Ui};
    let (w, h) = (240_u32, 200_u32);
    // Retina: fractional offsets land between device pixels.
    let mut gpu = GpuRenderer::new(w * 2, h * 2).unwrap();
    gpu.set_scale_factor(2.);
    // Present as macOS does: damage drawn into the drawable and the next
    // retained target in one pass.
    gpu.debug_enable_single_pass();
    let mut ui = Ui::new(w as f32, h as f32);
    let (fonts, cache) = (gpu.text_system(), gpu.text_cache());
    ui.scene
        .borrow_mut()
        .set_rich_text_measurer(move |rich, width| {
            cache
                .borrow_mut()
                .measure(&mut fonts.borrow_mut(), rich, width)
        });
    const WORDS: &[&str] = &["alpha", "streams", "a", "token", "while", "rows", "measure"];
    let heights = VariableHeights::new(&ui.runtime, 80, 90.).anchor_end();
    let offset = ui.signal(1e9_f32);
    // The last row holds a working spinner whose dots fade every frame.
    let tick = ui.signal(0_u32);
    let ticks = tick.clone();
    ui.mount(
        measured_virtual_list(
            offset.clone(),
            heights.clone(),
            2,
            |i| i,
            move |_, i, _| {
                if i == 79 {
                    let tick = ticks.clone();
                    return row().pt(16.).pb(40.).px(12.).gap(1.25).children((0..9).map(
                        move |dot| {
                            let tick = tick.clone();
                            div()
                                .size(2.5, 2.5)
                                .rounded(1.25)
                                .bg(rgb(0xb6d3ef))
                                .reactive_style(move || {
                                    Styles::new()
                                        .opacity(0.1 + ((tick.get() + dot) % 9) as f32 / 10.)
                                })
                        },
                    ));
                }
                let text: Vec<&str> = (0..(i * 7) % 23 + 1)
                    .map(|k| WORDS[(i + k) % WORDS.len()])
                    .collect();
                let paragraph = |text: String| {
                    rich_text()
                        .child(text_span(text).text_color(rgb(0xe8e8ea)))
                        .w(0.)
                        .grow()
                        .text_wrap(true)
                        .text_size(13.)
                        .line_height(19.)
                };
                // Markdown list items: a bullet centred on the first line.
                let items =
                    (0..i % 4).map(|n| {
                        row()
                            .w_full()
                            .gap(8.)
                            .items_start()
                            .mt(4.)
                            .child(
                                row().w(18.).h(19.).shrink_0().items_center().child(
                                    div().size(5., 5.).ml(1.).rounded(2.5).bg(rgb(0x8b7cf6)),
                                ),
                            )
                            .child(paragraph(WORDS[..(n * 3 + i) % WORDS.len() + 1].join(" ")))
                    });
                column()
                    .w_full()
                    .px(12.)
                    .mt(if i % 3 == 0 { 14. } else { 6. })
                    .child(
                        row()
                            .w_full()
                            .child(paragraph(format!("{i}: {}", text.join(" ")))),
                    )
                    .children(items)
            },
        )
        .fade_edges(20., 24.)
        .scrollbar(true)
        .size(w as f32, h as f32)
        .bg(rgb(0x202124)),
    );
    let full = Rect::new(0., 0., w as f32, h as f32);
    let mut previous: Option<Vec<u8>> = None;
    let mut frame = |ui: &mut Ui, label: &str| {
        tick.update(|t| *t += 1);
        ui.prepare_frame();
        let damage = ui.scene.borrow_mut().flush().damage;
        let moved: Vec<Rect> = ui
            .scene
            .borrow()
            .scroll_moves()
            .iter()
            .map(|m| m.clip)
            .collect();
        gpu.render(&ui.scene.borrow(), &damage).unwrap();
        let partial = gpu.debug_present_offscreen().unwrap();
        gpu.render(&ui.scene.borrow(), &[full]).unwrap();
        let fresh = gpu.debug_present_offscreen().unwrap();
        // Every pixel that changed since the last frame lies in the damage.
        if let Some(previous) = previous.replace(fresh.clone()) {
            let uncovered =
                previous
                    .chunks(4)
                    .zip(fresh.chunks(4))
                    .enumerate()
                    .find(|(p, (a, b))| {
                        let (x, y) = (
                            (p % (w as usize * 2)) as f32 / 2.,
                            (p / (w as usize * 2)) as f32 / 2.,
                        );
                        a != b
                            && !damage.iter().chain(&moved).any(|r| {
                                x >= r.x && y >= r.y && x < r.x + r.width && y < r.y + r.height
                            })
                    });
            if let Some((p, _)) = uncovered {
                let (x, y) = (
                    (p % (w as usize * 2)) as f32 / 2.,
                    (p / (w as usize * 2)) as f32 / 2.,
                );
                panic!("{label}: ({x}, {y}) changed outside the damage {damage:?}");
            }
        }
        let wrong = partial
            .chunks(4)
            .zip(fresh.chunks(4))
            .position(|(a, b)| a != b);
        assert!(
            wrong.is_none(),
            "{label}: first stale device pixel at {:?}",
            wrong.map(|p| (p % (w as usize * 2), p / (w as usize * 2)))
        );
    };
    frame(&mut ui, "open");
    frame(&mut ui, "measured");
    let mut position = offset.get();
    // Resampled trackpad input lands on fractional offsets.
    for (step, delta) in [
        -37., -150., -9.5, -400.25, -3.75, -900., -61.33, -1200., 250.5, -2000., 700., -5000.,
    ]
    .into_iter()
    .chain((0..40).map(|i| -17.3 - (i % 7) as f32 * 11.9))
    .enumerate()
    {
        position = (position + delta).max(0.);
        offset.set(position);
        frame(&mut ui, &format!("step {step} to {position}"));
        position = offset.get();
    }
}

/// The comparison benchmark's stream: a plain text whose 7-line window slides
/// every frame, so no text repeats. Prepared text must stay the size of what
/// is drawn, not grow with every text ever shown.
#[test]
fn sliding_stream_keeps_prepared_text_to_what_is_drawn() {
    let _gpu_fixture = gpu_fixture();
    use std::fmt::Write;
    use zgx::{compose::prelude::*, widgets::Ui};
    let mut gpu = GpuRenderer::new(960, 400).unwrap();
    let mut ui = Ui::new(960., 400.);
    gpu.install_text(&mut ui.scene.borrow_mut());
    let stream = ui.signal(String::new());
    let read = stream.clone();
    ui.mount(
        overlay().w(960.).h(400.).child(
            text_signal(move || read.get())
                .w(904.)
                .h(144.)
                .text_wrap(true),
        ),
    );
    let mut source = String::new();
    let mut peak = 0;
    for tick in 0..1000 {
        write!(source, "{tick:06} The quick brown fox streams a token. ").unwrap();
        let tail = &source.as_bytes()[source.len().saturating_sub(735)..];
        let visible = tail
            .chunks(105)
            .map(|chunk| std::str::from_utf8(chunk).unwrap())
            .collect::<Vec<_>>()
            .join("\n");
        stream.set(visible);
        ui.prepare_frame();
        let damage = ui.scene.borrow_mut().flush().damage;
        gpu.render(&ui.scene.borrow(), &damage).unwrap();
        peak = peak.max(gpu.text_cache().borrow().bytes().0);
    }
    // One drawn text of ~740 bytes, plus the 512 KB kept of the rest.
    assert!(peak < 1024 * 1024, "prepared text grew to {peak} bytes");
}

/// Plain quads draw with a lighter shader, and opaque flat ones without
/// blending (every run on a software rasterizer, large ones on a GPU; see
/// `debug_split_shading`): every feature must look the same either way, to
/// the byte.
#[test]
fn split_shading_matches_the_full_shader() {
    let _gpu_fixture = gpu_fixture();
    use std::sync::Arc;
    use zgx::{compose::prelude::*, widgets::Ui};
    let icon = Arc::new(
        zgx::svg::SvgData::new(
            &br##"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><circle cx="2" cy="2" r="1.6" fill="white" fill-opacity="0.7"/></svg>"##[..],
        )
        .unwrap()
        .tinted(rgb(0x88ccff)),
    );
    let pixels: Vec<u8> = (0..16 * 16)
        .flat_map(|i| {
            [
                (i * 7) as u8,
                (i * 3) as u8,
                200,
                if i % 3 == 0 { 128 } else { 255 },
            ]
        })
        .collect();
    let photo = Arc::new(zgx::image::ImageData::new(16, 16, pixels).unwrap());
    let view = || {
        column()
            .w(300.)
            .h(260.)
            .p(10.)
            .gap(6.)
            .bg(rgb(0x14161c))
            .text_color(rgb(0xe8e8ea))
            .child(text("Plain text over an opaque background").text_size(13.))
            .child(
                row()
                    .gap(6.)
                    .items_center()
                    .child(svg("icon", icon.clone()).size(14., 14.))
                    .child(
                        text("icon and label")
                            .text_size(12.)
                            .text_color(rgba(0xffffffb0)),
                    )
                    .child(image("photo", photo.clone()).size(24., 24.)),
            )
            .child(div().w(120.).h(20.).bg(rgba(0x3366ff80)))
            // Square panels: on whole pixels, and between them.
            .child(div().w(80.).h(12.).bg(rgb(0x55aa33)).border(0.))
            .child(div().w(33.3).h(10.7).ml(0.4).bg(rgb(0xaa5533)))
            .child(
                div()
                    .w(140.)
                    .h(34.)
                    .rounded(8.)
                    .border(1.)
                    .border_color(rgb(0x99aabb))
                    .bg(rgba(0x2a2f3acc))
                    .shadow(zgx::scene::BoxShadow {
                        color: rgba(0x00000080),
                        offset: zgx::scene::Transform { x: 0., y: 2. },
                        blur_radius: 6.,
                        spread: 0.,
                    })
                    .child(
                        rich_text()
                            .child(text_span("rich ").font_weight(700))
                            .child(text_span("spans").text_color(rgb(0xffaa44))),
                    ),
            )
            .child(
                column()
                    .h(60.)
                    .w(200.)
                    .fade_edges(10., 10.)
                    .overflow_hidden()
                    .children((0..5).map(|i| text(format!("faded row {i}")).text_size(11.))),
            )
            .child(text("rotated").text_size(12.).opacity(0.6))
            // A text's glyphs alone draw with one shading: faded at its
            // edges, in a rounded clip, or underlined, each as the full
            // shader draws it.
            .child(
                row()
                    .gap(6.)
                    .child(text("edge faded").text_size(12.).edge_fade(5.))
                    .child(
                        div()
                            .w(70.)
                            .h(16.)
                            .rounded(7.)
                            .overflow_hidden()
                            .child(text("in a rounded clip").text_size(12.)),
                    )
                    .child(text("underlined").text_size(12.).text_underline(true)),
            )
    };
    let render = |split: bool| {
        let mut gpu = GpuRenderer::new(600, 520).unwrap();
        gpu.set_scale_factor(2.);
        gpu.debug_split_shading(split);
        let mut ui = Ui::new(300., 260.);
        gpu.install_text(&mut ui.scene.borrow_mut());
        ui.mount(view());
        ui.prepare_frame();
        let damage = ui.scene.borrow_mut().flush().damage;
        gpu.render(&ui.scene.borrow(), &damage).unwrap();
        gpu.readback().unwrap()
    };
    let (full, split) = (render(false), render(true));
    let worst = full
        .iter()
        .zip(&split)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap();
    // Premultiplied samples round once less: at most one step.
    assert_eq!(worst, 0, "split shading differs by up to {worst}");
}

/// A square box on whole pixels draws the same bytes through either
/// shader, its corners included wherever they fall: a frame batches it
/// with whatever shares its region, so otherwise its pixels would hang on
/// what else repainted.
#[test]
fn a_square_box_on_whole_pixels_shades_its_corners_whole() {
    let _gpu_fixture = gpu_fixture();
    use zgx::{compose::prelude::*, widgets::Ui};
    let render = |split: bool| {
        let mut gpu = GpuRenderer::new(200, 100).unwrap();
        gpu.set_scale_factor(2.);
        gpu.debug_split_shading(split);
        let mut ui = Ui::new(100., 50.);
        ui.mount(
            div().w(100.).h(50.).bg(rgb(0x101418)).child(
                // Corners on odd device pixels.
                div()
                    .absolute()
                    .ml(10.5)
                    .mt(10.5)
                    .size(40., 12.)
                    .bg(rgb(0x5eb1ff)),
            ),
        );
        ui.prepare_frame();
        let damage = ui.scene.borrow_mut().flush().damage;
        gpu.render(&ui.scene.borrow(), &damage).unwrap();
        gpu.readback().unwrap()
    };
    let (full, split) = (render(false), render(true));
    // The top-left corner pixel (21, 21) is the box's colour.
    let at = (21 * 200 + 21) * 4;
    assert_eq!(&full[at..at + 3], &[0x5e, 0xb1, 0xff]);
    assert!(full == split, "split shading differs");
}

/// Clamped labels moving by a row (a mail list after archiving one) take
/// their shapes from the labels that showed their text before: the same
/// pixels as a renderer drawing that frame afresh.
#[test]
fn clamped_labels_moving_by_a_row_draw_as_afresh() {
    let _gpu_fixture = gpu_fixture();
    use zgx::{compose::prelude::*, widgets::Ui};
    let subjects: Vec<String> = (0..24)
        .map(|i| format!("Message {i}: a subject long enough to be cut short at the end"))
        .collect();
    let first = std::rc::Rc::new(std::cell::Cell::new(0));
    let view = |ui: &mut Ui, first: std::rc::Rc<std::cell::Cell<usize>>| {
        let offset = ui.signal(first.get());
        let rows = (0..12).map(|row| {
            let (offset, subjects) = (offset.clone(), subjects.clone());
            text_signal(move || subjects[offset.get() + row].clone())
                .text_size(12.)
                .truncate()
                .w(180.)
                .h(16.)
        });
        ui.mount(column().w(320.).h(240.).bg(rgb(0x14161c)).children(rows));
        offset
    };
    let mut gpu = GpuRenderer::new(640, 480).unwrap();
    gpu.set_scale_factor(2.);
    let mut ui = Ui::new(320., 240.);
    gpu.install_text(&mut ui.scene.borrow_mut());
    let offset = view(&mut ui, first.clone());
    for shift in 0..3 {
        offset.set(shift);
        ui.prepare_frame();
        let damage = ui.scene.borrow_mut().flush().damage;
        gpu.render(&ui.scene.borrow(), &damage).unwrap();
    }
    let moved = gpu.readback().unwrap();
    first.set(2);
    let mut fresh = GpuRenderer::new(640, 480).unwrap();
    fresh.set_scale_factor(2.);
    let mut afresh = Ui::new(320., 240.);
    fresh.install_text(&mut afresh.scene.borrow_mut());
    view(&mut afresh, first);
    afresh.prepare_frame();
    let damage = afresh.scene.borrow_mut().flush().damage;
    fresh.render(&afresh.scene.borrow(), &damage).unwrap();
    let differing = moved
        .chunks(4)
        .zip(fresh.readback().unwrap().chunks(4))
        .filter(|(a, b)| a != b)
        .count();
    assert_eq!(differing, 0, "{differing} pixels differ");
}

/// Small neighbours of different shading (a rounded chip, then a plain bar)
/// join draw calls, the full shader drawing the plain ones: a draw costs
/// the driver more than shading their few pixels. A plain box beside a
/// rounded one keeps its own draw: the full shader would cost the GPU more
/// over its thousands of pixels than the draw call saves.
#[test]
fn small_quads_of_different_shading_share_a_draw() {
    let _gpu_fixture = gpu_fixture();
    use zgx::{compose::prelude::*, widgets::Ui};
    let draws = |view: View| {
        let mut gpu = GpuRenderer::new(600, 400).unwrap();
        gpu.set_scale_factor(2.);
        let mut ui = Ui::new(300., 200.);
        gpu.install_text(&mut ui.scene.borrow_mut());
        ui.mount(view);
        ui.prepare_frame();
        let damage = ui.scene.borrow_mut().flush().damage;
        gpu.render(&ui.scene.borrow(), &damage).unwrap().draw_calls
    };
    let small = draws(row().gap(2.).children((0..20).map(|i| {
        let chip = div().size(6., 6.).rounded(2.).bg(rgb(0x3366aa));
        let bar = div().size(4., 10. + i as f32).bg(rgb(0xaa6633));
        row().child(chip).child(bar)
    })));
    // Forty alternating quads, one draw each before.
    assert!(small <= 10, "{small} draw calls");
    let large = draws(
        column()
            .child(div().size(40., 40.).rounded(6.).bg(rgb(0x3366aa)))
            .child(div().size(40., 40.).bg(rgb(0xaa6633))),
    );
    // The background, the rounded box, the plain one (6,400 pixels).
    assert_eq!(large, 3);
}

/// Opaque rounded and bordered panels draw a flat interior with the basic
/// shader and only their edge bands with the full one: the same bytes as one
/// quad, at fractional positions and sizes, per-corner radii and any scale.
#[test]
fn split_panels_match_whole_panels() {
    let _gpu_fixture = gpu_fixture();
    use zgx::{compose::prelude::*, widgets::Ui};
    let view = || {
        overlay()
            .w(320.)
            .h(240.)
            .bg(rgb(0x14161c))
            .children((0..24).map(|i| {
                let f = i as f32;
                let panel = div()
                    .absolute()
                    .ml(4.3 + (i % 6) as f32 * 52.7)
                    .mt(5.6 + (i / 6) as f32 * 57.9)
                    .w(40. + (f * 1.37) % 9.)
                    .h(36. + (f * 2.11) % 13.)
                    .bg(rgb(0x2a2f3a + i * 0x030507));
                match i % 4 {
                    0 => panel.rounded(6. + f % 5.),
                    1 => panel.rounded(3.5).border(1.).border_color(rgb(0x99aabb)),
                    2 => panel.border(2.5).border_color(rgba(0xffcc8880)),
                    _ => panel.rounded_corners(zgx::decoration::Corners {
                        top_left: 10.,
                        top_right: 0.,
                        bottom_right: 4.5,
                        bottom_left: 7.,
                    }),
                }
                .child(text("x").text_size(9.))
            }))
    };
    for scale in [1., 1.5, 2.] {
        let render = |split: bool| {
            let mut gpu = GpuRenderer::new((320. * scale) as u32, (240. * scale) as u32).unwrap();
            gpu.set_scale_factor(scale);
            gpu.debug_split_shading(split);
            let mut ui = Ui::new(320., 240.);
            gpu.install_text(&mut ui.scene.borrow_mut());
            ui.mount(view());
            ui.prepare_frame();
            let damage = ui.scene.borrow_mut().flush().damage;
            gpu.render(&ui.scene.borrow(), &damage).unwrap();
            gpu.readback().unwrap()
        };
        let (whole, split) = (render(false), render(true));
        let differing = whole
            .chunks(4)
            .zip(split.chunks(4))
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!(differing, 0, "{differing} pixels differ at scale {scale}");
    }
}

/// Changing text repaints where its glyphs land, not its whole box, and
/// repairs every old glyph: growing, shrinking, wrapping, aligned and italic.
#[test]
fn text_damage_covers_its_glyphs_and_matches_full_repaint() {
    let _gpu_fixture = gpu_fixture();
    use zgx::{compose::prelude::*, text_layout::TextAlign, widgets::Ui};
    let mut gpu = GpuRenderer::new(640, 480).unwrap();
    gpu.set_scale_factor(2.);
    let mut ui = Ui::new(320., 240.);
    gpu.install_text(&mut ui.scene.borrow_mut());
    let content = ui.signal("stream".to_owned());
    let texts: Vec<View> = [TextAlign::Left, TextAlign::Center, TextAlign::Right]
        .into_iter()
        .enumerate()
        .map(|(i, align)| {
            let content = content.clone();
            text_signal(move || content.get())
                .w(300.)
                .h(60.)
                .text_wrap(i == 0)
                .italic(i == 2)
                .text_size(15.)
                .text_align(align)
                .id(format!("t{i}"))
        })
        .collect();
    let view = ui.mount(
        column()
            .w(320.)
            .h(240.)
            .p(10.)
            .bg(rgb(0x1e2230))
            .text_color(rgb(0xffffff))
            .children(texts),
    );
    let full = zgx::scene::Rect::new(0., 0., 320., 240.);
    let mut frame = |ui: &mut Ui| {
        ui.prepare_frame();
        let damage = ui.scene.borrow_mut().flush().damage;
        gpu.render(&ui.scene.borrow(), &damage).unwrap();
        let partial = gpu.readback().unwrap();
        gpu.render(&ui.scene.borrow(), &[full]).unwrap();
        assert_eq!(partial, gpu.readback().unwrap(), "{:?}", damage);
        damage
    };
    frame(&mut ui);
    for next in [
        "stream grows longer with every token",
        "short",
        "A wrapped paragraph long enough to take a second and a third line in its box of three hundred pixels",
        "fjgqy WAVE ÅÄÖ",
        "",
        "back",
    ] {
        content.set(next.to_owned());
        let damage = frame(&mut ui);
        if next == "short" {
            // Only where glyphs were and are: far less than three boxes.
            let area: f32 = damage.iter().map(|r| r.width * r.height).sum();
            assert!(
                area < 3. * 300. * 60. * 0.8,
                "damaged {area} px: {damage:?}"
            );
        }
    }
    let _ = view;
}

#[test]
fn wide_backdrop_blur_matches_a_full_size_gaussian() {
    // A 16 px blur at 2x (sigma 32) runs averaged down; across a hard edge
    // it should still read as the exact blur, clamped inside the area.
    let _gpu_fixture = gpu_fixture();
    let mut gpu = GpuRenderer::new(512, 128).unwrap();
    gpu.set_scale_factor(2.);
    let mut s = Scene::new(256., 64.);
    s.set_kind(s.root(), NodeKind::Container(Layout::Overlay));
    s.append(
        s.root(),
        NodeKind::Rect(Color(0, 0, 0, 255)),
        fixed(256., 64.),
    );
    s.append(
        s.root(),
        NodeKind::Rect(Color(255, 255, 255, 255)),
        fixed(128., 64.),
    );
    let glass = s.append(
        s.root(),
        NodeKind::Rect(Color(0, 0, 0, 0)),
        fixed(256., 64.),
    );
    s.set_effects(
        glass,
        Effects {
            blur_radius: 16.,
            ..Default::default()
        },
    );
    let d = s.flush().damage;
    gpu.render(&s, &d).unwrap();
    let pixels = gpu.readback().unwrap();
    let sigma = 32_f64;
    let value = |x: i64| if x.clamp(0, 511) < 256 { 255. } else { 0. };
    let worst = (0..512_i64)
        .map(|x| {
            let (mut sum, mut weight) = (0., 0.);
            for i in -96..=96_i64 {
                let w = (-(i * i) as f64 / (2. * sigma * sigma)).exp();
                sum += value(x + i) * w;
                weight += w;
            }
            let got = pixels[((64 * 512 + x) * 4) as usize] as f64;
            (got - sum / weight).abs()
        })
        .fold(0_f64, f64::max);
    assert!(
        worst <= 2.,
        "within 2/255 of the exact blur, was {worst:.2}"
    );
}

/// Equal within a unit: GPUs round halves differently.
fn near(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(a, b)| a.abs_diff(*b) <= 1)
}

#[test]
fn text_lands_on_whole_device_pixels() {
    // A hinted glyph's one-pixel stroke drawn half a pixel off its atlas
    // texels is sampled between them: two rows at half strength, as a
    // minus sign at 1x read. Text snaps to the device pixel grid, as a
    // browser's does: placed 0.3 off a pixel it draws as on it, 0.7 off as
    // on the next.
    let _gpu_fixture = gpu_fixture();
    let draw = |x: f32, y: f32| {
        let mut gpu = GpuRenderer::new(80, 40).unwrap();
        let mut scene = Scene::new(80., 40.);
        scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
        let text = scene.append(
            scene.root(),
            NodeKind::Text {
                text: "-+H".into(),
                font_size: 13.,
                color: Color(255, 255, 255, 255),
            },
            fixed(60., 20.),
        );
        scene.set_transform(text, Transform { x, y });
        let damage = scene.flush().damage;
        gpu.render(&scene, &damage).unwrap();
        gpu.readback().unwrap()
    };
    let on = draw(10., 10.);
    assert!(on.chunks(4).any(|p| p[3] == 255), "drawn");
    assert!(on == draw(10.3, 10.3), "0.3 off draws as on the pixel");
    assert!(draw(11., 11.) == draw(10.7, 10.7), "0.7 off as on the next");
}

/// A long text scrolled in a clipping view makes quads for the lines that
/// reach the clip alone: the same pixels as making every line's, with far
/// fewer quads, and a document of 100,000 lines draws within the frame's
/// geometry budget.
#[test]
fn a_long_text_draws_only_the_lines_in_its_clip() {
    let _gpu_fixture = gpu_fixture();
    use zgx::{compose::prelude::*, widgets::Ui};
    let render = |lines: usize, cull: bool| {
        let mut gpu = GpuRenderer::new(400, 300).unwrap();
        gpu.set_scale_factor(2.);
        gpu.debug_cull_glyphs(cull);
        let mut ui = Ui::new(200., 150.);
        gpu.install_text(&mut ui.scene.borrow_mut());
        let text: String = (0..lines)
            .map(|i| format!("line {i} of the document\n"))
            .collect();
        ui.mount(
            overlay().w(180.).h(120.).overflow_hidden().child(
                zgx::compose::text(text)
                    .text_size(12.)
                    .line_height(15.)
                    .text_color(Color(255, 255, 255, 255))
                    .w(180.)
                    .h(lines as f32 * 15.)
                    // A line across the clip's top edge: no quad culled whole.
                    .translate(0., -12_007.),
            ),
        );
        ui.prepare_frame();
        let damage = ui.scene.borrow_mut().flush().damage;
        let stats = gpu.render(&ui.scene.borrow(), &damage).unwrap();
        (gpu.readback().unwrap(), stats.instances)
    };
    let (culled, few) = render(2_000, true);
    let (whole, all) = render(2_000, false);
    assert!(culled == whole, "culled lines change pixels");
    assert!(
        culled.iter().any(|byte| *byte > 200),
        "the visible lines draw"
    );
    assert!(few * 20 < all, "{few} quads culled, {all} whole");
    let (_, huge) = render(100_000, true);
    assert!(huge < 2_000, "{huge} quads for a 100,000-line document");
}

#[test]
fn find_marks_are_rounded_whichever_shader_draws_the_text() {
    let _gpu_fixture = gpu_fixture();
    let draw = |split| {
        let mut gpu = GpuRenderer::new(80, 40).unwrap();
        gpu.debug_split_shading(split);
        let mut scene = Scene::new(80., 40.);
        scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
        let node = scene.append(
            scene.root(),
            NodeKind::Text {
                text: "x".into(),
                font_size: 13.,
                color: Color(255, 255, 255, 255),
            },
            fixed(60., 28.),
        );
        scene.set_text_marks(
            node,
            vec![TextMark {
                rect: Rect::new(24., 4., 20., 16.),
                color: Color(255, 180, 20, 255),
            }],
        );
        let damage = scene.flush().damage;
        gpu.render(&scene, &damage).unwrap();
        gpu.readback().unwrap()
    };
    let full = draw(false);
    let split = draw(true);
    let changed = full
        .as_chunks::<4>()
        .0
        .iter()
        .zip(split.as_chunks::<4>().0)
        .filter(|(a, b)| a != b)
        .count();
    assert_eq!(changed, 0, "find marks differ from the full shader's");
}

/// A page of large text grows the glyph atlas past its first size. A pause
/// keeps it (the next frame would rasterize everything again); a long idle
/// gives it back, and the next frame draws the same picture, the atlas back
/// at the size it had in one step.
#[test]
fn a_grown_atlas_goes_back_at_idle_and_draws_the_same() {
    let _gpu = gpu_fixture();
    let mut scene = Scene::new(1200., 900.);
    let lines: Vec<String> = (0..5)
        .map(|line| (0..14).map(|i| (b'A' + line * 5 + i % 5) as char).collect())
        .collect();
    for (i, text) in lines.iter().enumerate() {
        scene.append(
            scene.root(),
            NodeKind::Text {
                text: text.as_str().into(),
                font_size: 120. + i as f32 * 7.,
                color: Color(255, 255, 255, 255),
            },
            fixed(1200., 170.),
        );
    }
    let mut gpu = GpuRenderer::new(1200, 900).unwrap();
    let damage = scene.flush().damage;
    gpu.render(&scene, &damage).unwrap();
    let first_size = 512 * 512 * 4;
    assert!(gpu.debug_cache_stats().atlas_bytes > first_size);
    let before = gpu.readback().unwrap();
    let grown = gpu.debug_cache_stats().atlas_bytes;
    gpu.trim();
    assert_eq!(gpu.debug_cache_stats().atlas_bytes, grown);
    assert!(gpu.atlas_grown());
    gpu.trim_atlas();
    assert_eq!(gpu.debug_cache_stats().atlas_bytes, first_size);
    let whole = [Rect::new(0., 0., 1200., 900.)];
    gpu.render(&scene, &whole).unwrap();
    assert_eq!(gpu.debug_cache_stats().atlas_bytes, grown);
    assert!(gpu.readback().unwrap() == before, "the same picture");
}

/// A rounded, bordered image keeps its corners and its frame however it is
/// fitted, on both of the renderer's ways of drawing an image: a portrait
/// picture cropped top and bottom by `cover` too.
#[test]
fn a_rounded_image_keeps_its_corners_and_frame_however_it_fits() {
    let _gpu_fixture = gpu_fixture();
    use std::sync::Arc;
    use zgx::{compose::prelude::*, image::ImageData, widgets::Ui};
    let pixels: Vec<u8> = [230u8, 40, 40, 255].repeat(60 * 200);
    let source = Arc::new(ImageData::new(60, 200, pixels).unwrap());
    let background = [16, 32, 48, 255];
    for (fit, layered, name) in [
        (ObjectFit::Cover, false, "cover"),
        (ObjectFit::Cover, true, "cover, in a layer"),
        (ObjectFit::Fill, false, "fill"),
        (ObjectFit::Fill, true, "fill, in a layer"),
        (ObjectFit::None, false, "none"),
    ] {
        let mut gpu = GpuRenderer::new(120, 120).unwrap();
        let mut ui = Ui::new(120., 120.);
        let bitmap = source.clone();
        ui.mount(div().size(120., 120.).bg(rgb(0x102030)).child({
            let view = image_signal("Portrait", move || bitmap.clone())
                .size(100., 100.)
                .m(10.)
                .rounded(24.)
                .border(2.)
                .border_color(rgb(0xffffff))
                .object_fit(fit);
            // Drawn into a layer of its own, then composited.
            if layered { view.opacity(0.999) } else { view }
        }));
        ui.prepare_frame();
        let damage = ui.scene.borrow_mut().flush().damage;
        gpu.render(&ui.scene.borrow(), &damage).unwrap();
        let pixels = gpu.readback().unwrap();
        let at = |x: usize, y: usize| {
            let i = (y * 120 + x) * 4;
            [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
        };
        // Outside the rounded corners: what is behind, at each corner.
        for (x, y) in [(12, 12), (107, 12), (12, 107), (107, 107)] {
            let near = at(x, y)
                .iter()
                .zip(background)
                .all(|(a, b)| a.abs_diff(b) <= 1);
            assert!(near, "{name}: corner at {x},{y}: {:?}", at(x, y));
        }
        // The frame, on every side, over the picture.
        for (x, y) in [(60, 11), (60, 108), (11, 60), (108, 60)] {
            assert!(
                at(x, y)[1] > 200,
                "{name}: frame at {x},{y}: {:?}",
                at(x, y)
            );
        }
        // And the picture within it.
        assert!(
            at(60, 60)[0] > 180 && at(60, 60)[1] < 110,
            "{name}: {:?}",
            at(60, 60)
        );
    }
}

/// An underline in each style, on a run of spaces so only the line shows: a
/// double has two rows, dots and dashes have gaps along it (dashes longer),
/// and a wave moves up and down, in the colour asked for, at both scales.
#[test]
fn underlines_draw_in_each_line_style() {
    use std::sync::Arc;
    use zgx::rich_text::{Decoration, LineStyle, RichText, TextRun};
    let _gpu_fixture = gpu_fixture();
    let red = Color(240, 20, 30, 255);
    for scale in [1u32, 2] {
        // The line's pixels in each column, top to bottom.
        let columns = |style: LineStyle| {
            let mut scene = Scene::new(200., 40.);
            let text = " ".repeat(40);
            let run = TextRun {
                range: 0..text.len(),
                underline: Some(Decoration::new(2.).color(red).line(style)),
                ..Default::default()
            };
            scene.append(
                scene.root(),
                NodeKind::RichText {
                    text: Arc::new(RichText::new(text, vec![run]).unwrap()),
                },
                fixed(200., 40.),
            );
            let mut gpu = GpuRenderer::new(200 * scale, 40 * scale).unwrap();
            gpu.set_scale_factor(scale as f32);
            let damage = scene.flush().damage;
            gpu.render(&scene, &damage).unwrap();
            let pixels = gpu.readback().unwrap();
            let (width, height) = (200 * scale as usize, 40 * scale as usize);
            (0..width)
                .map(|x| {
                    (0..height)
                        .filter(|y| {
                            let p = &pixels[(y * width + x) * 4..][..4];
                            p[0] > 120 && p[1] < 110 && p[2] < 110
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        };
        let drawn = |columns: &[Vec<usize>]| columns.iter().filter(|c| !c.is_empty()).count();
        // Runs of columns with ink.
        let pieces = |columns: &[Vec<usize>]| {
            columns
                .windows(2)
                .filter(|w| w[0].is_empty() && !w[1].is_empty())
                .count()
                + usize::from(!columns[0].is_empty())
        };
        let solid = columns(LineStyle::Solid);
        assert!(
            drawn(&solid) > 50,
            "scale {scale}: no line: {}",
            drawn(&solid)
        );
        assert_eq!(pieces(&solid), 1, "scale {scale}");
        let thick = solid.iter().map(Vec::len).max().unwrap();
        let double = columns(LineStyle::Double);
        assert_eq!(pieces(&double), 1, "scale {scale}");
        assert!(double.iter().map(Vec::len).max().unwrap() >= 2 * thick);
        let (dotted, dashed) = (columns(LineStyle::Dotted), columns(LineStyle::Dashed));
        assert!(
            pieces(&dotted) > 6,
            "scale {scale}: {} dots",
            pieces(&dotted)
        );
        assert!(
            pieces(&dashed) > 3,
            "scale {scale}: {} dashes",
            pieces(&dashed)
        );
        assert!(pieces(&dashed) < pieces(&dotted));
        assert!(
            drawn(&dashed) * 100 > drawn(&dotted) * 110,
            "dashes cover more"
        );
        let wavy = columns(LineStyle::Wavy);
        assert_eq!(pieces(&wavy), 1, "scale {scale}: a wave doesn't break");
        let tops: Vec<usize> = wavy.iter().filter_map(|c| c.first().copied()).collect();
        let (low, high) = (tops.iter().min().unwrap(), tops.iter().max().unwrap());
        assert!(high - low >= 2, "scale {scale}: it moves: {low} to {high}");
    }
}

/// `object_position` places what `cover` crops: a portrait's top, its middle
/// (the default) or its bottom shows, and a `none` picture its corner.
#[test]
fn object_position_says_what_a_crop_keeps() {
    use std::sync::Arc;
    use zgx::{compose::prelude::*, image::ImageData, widgets::Ui};
    let _gpu_fixture = gpu_fixture();
    // A portrait in thirds: red, green, blue, top to bottom.
    let (red, green, blue) = ([230, 30, 40, 255], [30, 210, 60, 255], [30, 70, 220, 255]);
    let pixels: Vec<u8> = (0..60)
        .flat_map(|y| {
            let band = [red, green, blue][y / 20];
            (0..20).flat_map(move |_| band)
        })
        .collect();
    let source = Arc::new(ImageData::new(20, 60, pixels).unwrap());
    let shown = |view: fn(View) -> View| {
        let (mut gpu, mut ui) = (GpuRenderer::new(100, 100).unwrap(), Ui::new(100., 100.));
        let bitmap = source.clone();
        ui.mount(view(
            image_signal("Portrait", move || bitmap.clone()).size(60., 60.),
        ));
        ui.prepare_frame();
        let damage = ui.scene.borrow_mut().flush().damage;
        gpu.render(&ui.scene.borrow(), &damage).unwrap();
        let pixels = gpu.readback().unwrap();
        move |x: usize, y: usize| -> [u8; 4] {
            pixels[(y * 100 + x) * 4..][..4].try_into().unwrap()
        }
    };
    let near = |a: [u8; 4], b: [u8; 4]| a.iter().zip(b).all(|(a, b)| a.abs_diff(b) <= 12);
    // Centred, as ever: the middle band fills the middle of the box.
    let middle = shown(|v| v.object_fit(ObjectFit::Cover));
    assert!(near(middle(30, 30), green), "{:?}", middle(30, 30));
    let top = shown(|v| v.object_fit(ObjectFit::Cover).object_position(0.5, 0.));
    assert!(near(top(30, 10), red), "{:?}", top(30, 10));
    let bottom = shown(|v| v.object_fit(ObjectFit::Cover).object_position(0.5, 1.));
    assert!(near(bottom(30, 50), blue), "{:?}", bottom(30, 50));
    // Uncropped: a picture smaller than its box, at its end.
    let end = shown(|v| v.object_fit(ObjectFit::None).object_position(1., 0.));
    assert!(near(end(50, 5), red), "{:?}", end(50, 5));
    assert!(!near(end(5, 5), red), "nothing at the other side");
}

/// The report's first frame: a 300 by 420 card in a zoomed box, its tilt 26.3°
/// and perspective 53.6, so its edge is 93 points toward an eye 53.6 away.
/// Its parent's layer holds what can show, and the frame draws.
#[test]
fn a_tilt_past_the_eye_inside_a_zoomed_layer_draws() {
    let _gpu_fixture = gpu_fixture();
    let (w, h) = (800., 900.);
    let mut gpu = GpuRenderer::new(w as u32, h as u32).unwrap();
    let mut scene = Scene::new(w, h);
    scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
    let zoomed = scene.append(
        scene.root(),
        NodeKind::Container(Layout::Overlay),
        fixed(w, h),
    );
    scene.set_zoom(zoomed, 0.76);
    scene.set_isolated(zoomed, true);
    let card = scene.append(
        zoomed,
        NodeKind::Rect(Color(200, 40, 40, 255)),
        fixed(300., 420.),
    );
    scene.set_transform(card, Transform { x: 240., y: 240. });
    scene.set_isolated(card, true);
    let tilt = |degrees: f32, perspective: f32| LocalTransform {
        rotate_x: degrees.to_radians(),
        perspective,
        ..LocalTransform::IDENTITY
    };
    scene.set_local_transform(card, tilt(26.3, 53.6));
    let bounds = scene.layer_bounds(zoomed);
    assert!(
        bounds.width <= w / 0.76 && bounds.height <= h / 0.76,
        "what can show, not everything: {bounds:?}"
    );
    let report = scene.flush();
    gpu.render(&scene, &report.damage).unwrap();
    assert_eq!(gpu.take_issues(), Vec::<String>::new());
    // At its distance, it fits where it always did.
    scene.set_local_transform(card, tilt(0.5, 900.));
    let bounds = scene.layer_bounds(zoomed);
    assert!(bounds.width <= w && bounds.height <= h, "{bounds:?}");
    let report = scene.flush();
    gpu.render(&scene, &report.damage).unwrap();
}

/// Past the budget or the texture size, a layer rasterizes coarser, is said
/// once, and shows.
#[test]
fn a_layer_past_its_budget_draws_coarser_and_says_so_once() {
    let _gpu_fixture = gpu_fixture();
    for (width, height) in [(12_000., 12_000.), (60_000., 40.)] {
        let mut gpu = GpuRenderer::new(100, 100).unwrap();
        let mut scene = Scene::new(100., 100.);
        scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
        let layer = scene.append(
            scene.root(),
            NodeKind::Rect(Color(220, 30, 30, 255)),
            fixed(width, height),
        );
        scene.set_isolated(layer, true);
        scene.set_effects(
            layer,
            Effects {
                opacity: 0.999,
                ..Default::default()
            },
        );
        let report = scene.flush();
        gpu.render(&scene, &report.damage).unwrap();
        let issues = gpu.take_issues();
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert!(issues[0].contains("coarser"), "{issues:?}");
        let pixels = gpu.readback().unwrap();
        assert!(pixels[(10 * 100 + 10) * 4] > 200, "it shows");
        scene.set_transform(layer, Transform { x: -1., y: 0. });
        let report = scene.flush();
        gpu.render(&scene, &report.damage).unwrap();
        assert!(gpu.take_issues().is_empty(), "said once");
    }
}

/// Past 1024 layers, or the budget of them all, frames go on.
#[test]
fn layers_past_their_count_and_budget_all_draw() {
    let _gpu_fixture = gpu_fixture();
    let mut gpu = GpuRenderer::new(600, 600).unwrap();
    let mut scene = Scene::new(600., 600.);
    scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
    for index in 0..1100 {
        let side = if index < 8 { 2000. } else { 8. };
        let layer = scene.append(
            scene.root(),
            NodeKind::Rect(Color(30, 200, 60, 255)),
            fixed(side, side),
        );
        scene.set_isolated(layer, true);
        scene.set_effects(
            layer,
            Effects {
                opacity: 0.999,
                ..Default::default()
            },
        );
    }
    let report = scene.flush();
    gpu.render(&scene, &report.damage).unwrap();
    let issues = gpu.take_issues();
    assert!(issues.iter().any(|i| i.contains("1024")), "{issues:?}");
    assert!(
        issues.iter().any(
            |i| i.contains("past the budget of isolated layers (64 MiB)")
                && i.contains("MiB allocated")
        ),
        "{issues:?}"
    );
    assert!(gpu.readback().unwrap()[(300 * 600 + 300) * 4 + 1] > 150);
}

/// An image past the texture size or the budget at full size is drawn at
/// the size it shows: nothing is left out, and nothing is said.
#[test]
fn images_too_large_for_a_texture_draw_at_the_size_they_show() {
    use std::sync::Arc;
    use zgx::image::ImageData;
    let _gpu_fixture = gpu_fixture();
    for (width, height) in [(5000, 4000), (40_000, 1)] {
        let mut gpu = GpuRenderer::new(100, 100).unwrap();
        let mut scene = Scene::new(100., 100.);
        scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
        let pixels = vec![255; width * height * 4];
        let image = Arc::new(ImageData::new(width as u32, height as u32, pixels).unwrap());
        scene.append(
            scene.root(),
            NodeKind::Rect(Color(30, 60, 220, 255)),
            fixed(100., 100.),
        );
        scene.append(scene.root(), NodeKind::Image(image), fixed(50., 50.));
        let report = scene.flush();
        gpu.render(&scene, &report.damage).unwrap();
        assert!(gpu.take_issues().is_empty());
        let pixels = gpu.readback().unwrap();
        assert!(pixels[(25 * 100 + 25) * 4] > 200, "the image shows");
        assert!(pixels[(75 * 100 + 75) * 4 + 2] > 200);
    }
}

/// The 33rd shadow on a panel is left off, once said, and the frame draws.
#[test]
fn a_panel_past_32_shadows_draws_the_first_32() {
    use zgx::{compose::prelude::*, widgets::Ui};
    let _gpu_fixture = gpu_fixture();
    let mut gpu = GpuRenderer::new(100, 100).unwrap();
    let mut ui = Ui::new(100., 100.);
    let shadow = BoxShadow {
        color: Color(0, 0, 0, 40),
        offset: Transform { x: 0., y: 2. },
        blur_radius: 2.,
        spread: 0.,
    };
    ui.mount(
        div()
            .size(40., 40.)
            .bg(Color(255, 255, 255, 255))
            .shadows(vec![shadow; 33]),
    );
    ui.prepare_frame();
    let damage = ui.scene.borrow_mut().flush().damage;
    gpu.render(&ui.scene.borrow(), &damage).unwrap();
    let issues = gpu.take_issues();
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert!(issues[0].contains("32 shadows"), "{issues:?}");
}

/// Where a tilted card shows and where it is hit are the same place: every
/// point of the window, drawn against hit, at zoom 1 and 0.76, a sibling
/// column beside it, under a scroll or not.
#[test]
fn a_tilted_card_is_hit_where_it_shows() {
    let _gpu_fixture = gpu_fixture();
    let (w, h) = (800usize, 600usize);
    let tilt = LocalTransform {
        rotate_x: 12f32.to_radians(),
        rotate_y: (-12f32).to_radians(),
        perspective: 900.,
        ..LocalTransform::IDENTITY
    };
    for (zoom, scrolled) in [(1., 0.), (0.76, 0.), (1., 40.), (0.76, 40.)] {
        let mut gpu = GpuRenderer::new(w as u32, h as u32).unwrap();
        let mut scene = Scene::new(w as f32, h as f32);
        scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
        let page = scene.append(
            scene.root(),
            NodeKind::Container(Layout::Overlay),
            fixed(w as f32, h as f32),
        );
        scene.set_transform(page, Transform { x: 30., y: 20. });
        scene.set_zoom(page, zoom);
        if zoom != 1. {
            scene.set_isolated(page, true);
        }
        let list = scene.append(
            page,
            NodeKind::Container(Layout::Overlay),
            fixed(w as f32, h as f32),
        );
        scene.set_transform(
            list,
            Transform {
                x: 0.,
                y: -scrolled,
            },
        );
        let card = scene.append(
            list,
            NodeKind::Rect(Color(220, 30, 30, 255)),
            fixed(200., 240.),
        );
        scene.set_transform(card, Transform { x: 80., y: 100. });
        scene.set_isolated(card, true);
        scene.set_local_transform(card, tilt);
        let next = scene.append(
            list,
            NodeKind::Rect(Color(30, 30, 220, 255)),
            fixed(200., 240.),
        );
        scene.set_transform(next, Transform { x: 400., y: 100. });
        let report = scene.flush();
        gpu.render(&scene, &report.damage).unwrap();
        let pixels = gpu.readback().unwrap();
        let mut wrong = Vec::new();
        for (x, y) in (0..h)
            .step_by(6)
            .flat_map(|y| (0..w).step_by(6).map(move |x| (x, y)))
        {
            let at = (y * w + x) * 4;
            let (red, blue) = (pixels[at], pixels[at + 2]);
            let hit = scene.hit_test_all(x as f32 + 0.5, y as f32 + 0.5);
            // The edges' antialiasing says nothing.
            if (40..200).contains(&red.max(blue)) {
                continue;
            }
            if (red > 200) != hit.contains(&card) || (blue > 200) != hit.contains(&next) {
                wrong.push((x, y, red, blue));
            }
        }
        assert!(
            wrong.len() <= 8,
            "zoom {zoom}, scrolled {scrolled}: {} points differ: {:?}",
            wrong.len(),
            &wrong[..wrong.len().min(8)]
        );
    }
}

/// A text's shadow paints its glyphs again, moved and in its colour, under
/// them; without one, nothing does. Moved, it leaves no trail: its old place
/// is damage.
#[test]
fn a_text_shadow_paints_under_its_glyphs_and_moves_without_a_trail() {
    use zgx::{compose::prelude::*, widgets::Ui};
    let _gpu_fixture = gpu_fixture();
    let red = |p: &[u8]| p[0] > 180 && p[1] < 80 && p[2] < 80;
    let draw = |shadow: bool| {
        let (mut gpu, mut ui) = (GpuRenderer::new(200, 100).unwrap(), Ui::new(200., 100.));
        let label = text("HHHH").text_size(40.).text_color(Color(0, 0, 0, 255));
        let label = match shadow {
            true => label.text_shadow(0., 30., 0., Color(230, 20, 20, 255)),
            false => label,
        };
        ui.mount(
            div()
                .size(200., 100.)
                .bg(Color(255, 255, 255, 255))
                .child(label),
        );
        ui.prepare_frame();
        let damage = ui.scene.borrow_mut().flush().damage;
        gpu.render(&ui.scene.borrow(), &damage).unwrap();
        let pixels = gpu.readback().unwrap();
        (pixels.chunks(4).filter(|p| red(p)).count(), ui)
    };
    assert_eq!(draw(false).0, 0, "no shadow, no red");
    let (count, ui) = draw(true);
    assert!(count > 200, "the shadow shows: {count} red pixels");
    // Its glyphs reach at most 50 down; its shadow 30 below that.
    let mut scene = ui.scene.borrow_mut();
    let node = scene
        .children(scene.root())
        .first()
        .and_then(|div| scene.children(*div).first().copied())
        .unwrap();
    scene.set_transform(node, Transform { x: 100., y: 0. });
    let damage = scene.flush().damage;
    assert!(
        damage.iter().any(|d| d.x <= 10. && d.y + d.height >= 75.),
        "its old shadow is damage: {damage:?}"
    );
}

/// The user's case: two large photos (4000 by 3000) shown as 300 by 420
/// cards at 2x. Each texture holds its card's pixels, not its file's (48 MB
/// each would pass the budget between them), both draw, and a card drawn
/// larger (zoomed) is uploaded again, sharper; smaller, it keeps its own.
#[test]
fn large_photos_in_small_cards_hold_their_cards_pixels() {
    use std::sync::Arc;
    use zgx::image::ImageData;
    let _gpu_fixture = gpu_fixture();
    let (w, h) = (4000usize, 3000usize);
    let photo = |shade: u8| {
        let pixels: Vec<u8> = (0..w * h)
            .flat_map(|p| {
                [
                    shade,
                    ((p % w) * 255 / w) as u8,
                    ((p / w) * 255 / h) as u8,
                    255,
                ]
            })
            .collect();
        Arc::new(ImageData::new(w as u32, h as u32, pixels).unwrap())
    };
    let mut gpu = GpuRenderer::new(1400, 1000).unwrap();
    gpu.set_scale_factor(2.);
    let mut scene = Scene::new(700., 500.);
    scene.set_kind(scene.root(), NodeKind::Container(Layout::Row));
    let card = |scene: &mut Scene, image| {
        scene.append(scene.root(), NodeKind::Image(image), fixed(300., 420.))
    };
    let first = card(&mut scene, photo(200));
    card(&mut scene, photo(40));
    let report = scene.flush();
    let stats = gpu.render(&scene, &report.damage).unwrap();
    assert!(gpu.take_issues().is_empty(), "both drawn");
    assert_eq!(stats.image_uploads, 2);
    // Stretched into 600 by 840 device pixels: held at 1120 by 840, its
    // shape kept (3.8 MB each, not 48: two at 48 pass the budget).
    let held = gpu.debug_cache_stats().image_bytes;
    assert_eq!(held, 2 * 1120 * 840 * 4, "{held}");
    // Its card's red, top left: the photo, not a gap.
    let pixels = gpu.readback().unwrap();
    assert!(pixels[(20 * 1400 + 20) * 4..][..3].iter().any(|&c| c > 150));
    // Drawn three times as large: uploaded again, larger (its layer
    // rasterizes at the nearest √2 step, 2.83).
    scene.set_local_transform(
        first,
        LocalTransform {
            scale: (3., 3.),
            ..LocalTransform::IDENTITY
        },
    );
    scene.set_isolated(first, true);
    let report = scene.flush();
    let stats = gpu.render(&scene, &report.damage).unwrap();
    assert_eq!(stats.image_uploads, 1, "the larger one alone");
    let zoomed = gpu.debug_cache_stats().image_bytes - 1120 * 840 * 4;
    assert!(
        zoomed > 3 * 1120 * 840 * 4 && zoomed < 4000 * 3000 * 4,
        "{zoomed}"
    );
    // Back down: kept as it is (no churn).
    scene.set_local_transform(first, LocalTransform::IDENTITY);
    let report = scene.flush();
    assert_eq!(gpu.render(&scene, &report.damage).unwrap().image_uploads, 0);
}

/// One layer as large as a 2x window (2880 by 1800, 20.7 MB) is drawn at
/// its scale, not coarser: a layer has no room of its own past what the
/// budget leaves (it had 16 MiB, and warned of the 64 MiB budget alone).
#[test]
fn a_window_sized_layer_draws_at_its_scale() {
    let _gpu_fixture = gpu_fixture();
    let mut gpu = GpuRenderer::new(2880, 1800).unwrap();
    gpu.set_scale_factor(2.);
    let mut scene = Scene::new(1440., 900.);
    scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
    let layer = scene.append(
        scene.root(),
        NodeKind::Rect(Color(200, 30, 30, 255)),
        fixed(1440., 900.),
    );
    scene.set_isolated(layer, true);
    scene.set_effects(
        layer,
        Effects {
            opacity: 0.999,
            ..Default::default()
        },
    );
    let report = scene.flush();
    gpu.render(&scene, &report.damage).unwrap();
    assert_eq!(gpu.take_issues(), Vec::<String>::new());
    // Its texture: 2882 by 1802 with its border, rounded up to 64s.
    assert_eq!(gpu.debug_cache_stats().layer_bytes, 2944 * 1856 * 4);
    // Each layer is listed: its node, its pixels, its scale.
    let mut report = zgx::memory::Report::default();
    gpu.memory(&mut report);
    let line = report
        .lines
        .iter()
        .find(|line| line.area == "gpu layer")
        .expect("listed");
    assert!(
        line.name
            .starts_with(&format!("@{} 2882x1802 px at 2.00x", layer.number())),
        "{}",
        line.name
    );
    assert_eq!(line.bytes, Some(2944 * 1856 * 4));
}
