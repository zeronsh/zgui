//! Outlines (CSS `outline`, `outline-offset`) and stroke alignment: a ring
//! outside (or, offset negative, across) a box's edge, its corners the
//! box's radii grown alike, over its content, not cut by its own clip,
//! taking no layout; on the wgpu path and with Metal directly.
use zgx::scene::*;
use zgx_gpu::GpuRenderer;

fn gpu_fixture() -> std::sync::MutexGuard<'static, ()> {
    static DEVICE_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());
    DEVICE_TEST
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
const SIZE: u32 = 200;
const WHITE: [u8; 4] = [255; 4];
const DARK: [u8; 4] = [20, 20, 20, 255];
const CLEAR: [u8; 4] = [0; 4];
fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let at = ((y * SIZE + x) * 4) as usize;
    pixels[at..at + 4].try_into().unwrap()
}
/// A dark 100 × 100 panel at (50, 50), rounded `radius`, clipping, with a
/// white outline of `width` at `offset`.
fn outlined(scene: &mut Scene, radius: f32, width: f32, offset: f32) -> NodeId {
    let root = scene.root();
    outlined_in(scene, root, radius, width, offset)
}
fn outlined_in(scene: &mut Scene, parent: NodeId, radius: f32, width: f32, offset: f32) -> NodeId {
    let id = scene.append(
        parent,
        NodeKind::Panel {
            layout: Layout::Overlay,
            quad: QuadStyle {
                fill: Color(20, 20, 20, 255),
                radius,
                outline: Some(Outline {
                    width,
                    color: Color(255, 255, 255, 255),
                    offset,
                }),
                ..Default::default()
            },
        },
        Style {
            width: Some(100.),
            height: Some(100.),
            clip: true,
            ..Default::default()
        },
    );
    scene.set_transform(id, Transform { x: 50., y: 50. });
    id
}
fn set_outline(scene: &mut Scene, id: NodeId, outline: Option<Outline>) {
    let NodeKind::Panel { layout, quad } = scene.kind(id).clone() else {
        unreachable!()
    };
    scene.set_kind(
        id,
        NodeKind::Panel {
            layout,
            quad: QuadStyle { outline, ..quad },
        },
    );
}
fn scene() -> Scene {
    let mut scene = Scene::new(SIZE as f32, SIZE as f32);
    scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
    scene
}
/// Render the flushed frame, then check it against a full repaint.
fn render(gpu: &mut GpuRenderer, scene: &mut Scene) -> (FrameReport, Vec<u8>) {
    let frame = scene.flush();
    gpu.render(scene, &frame.damage).unwrap();
    let pixels = gpu.readback().unwrap();
    gpu.render(scene, &[Rect::new(0., 0., SIZE as f32, SIZE as f32)])
        .unwrap();
    let full = gpu.readback().unwrap();
    let differs: Vec<_> = (0..SIZE * SIZE)
        .filter(|&at| pixels[at as usize * 4..][..4] != full[at as usize * 4..][..4])
        .map(|at| (at % SIZE, at / SIZE))
        .collect();
    assert!(
        differs.is_empty(),
        "incremental frame differs from full at {} pixels, first {:?} ({:?} vs {:?}), damage {:?}",
        differs.len(),
        &differs[..differs.len().min(4)],
        differs.first().map(|&(x, y)| pixel(&pixels, x, y)),
        differs.first().map(|&(x, y)| pixel(&full, x, y)),
        frame.damage
    );
    (frame, pixels)
}
fn renderers() -> impl Iterator<Item = (bool, GpuRenderer)> {
    [true, false].into_iter().map(|direct| {
        let mut gpu = GpuRenderer::new(SIZE, SIZE).unwrap();
        gpu.set_direct_metal(direct);
        (direct, gpu)
    })
}

#[test]
fn outlines_ring_the_box_at_their_offset_without_layout() {
    let _gpu_fixture = gpu_fixture();
    for (direct, mut gpu) in renderers() {
        // 4 wide, 2 out: the ring spans 6 to 2 px outside the box, square
        // corners square, the box's own clip notwithstanding.
        let mut scene = scene();
        let id = outlined(&mut scene, 0., 4., 2.);
        let (_, p) = render(&mut gpu, &mut scene);
        assert_eq!(pixel(&p, 45, 100), WHITE, "direct {direct}");
        assert_eq!(pixel(&p, 49, 100), CLEAR, "the gap of its offset");
        assert_eq!(pixel(&p, 51, 100), DARK);
        assert_eq!(pixel(&p, 154, 100), WHITE);
        assert_eq!(pixel(&p, 45, 45), WHITE, "a square corner stays square");
        assert_eq!(scene.bounds(id), Rect::new(50., 50., 100., 100.));
        // Rounded 10: the ring's corner curves at 16 outside, 12 inside.
        let mut scene = self::scene();
        outlined(&mut scene, 10., 4., 2.);
        let (_, p) = render(&mut gpu, &mut scene);
        assert_eq!(pixel(&p, 45, 45), CLEAR);
        assert_eq!(pixel(&p, 45, 100), WHITE);
        // Moving and removing it repaints what it covered; no layout.
        let mut scene = self::scene();
        let id = outlined(&mut scene, 0., 4., 2.);
        render(&mut gpu, &mut scene);
        set_outline(
            &mut scene,
            id,
            Some(Outline {
                width: 4.,
                color: Color(255, 255, 255, 255),
                offset: 8.,
            }),
        );
        let (frame, p) = render(&mut gpu, &mut scene);
        assert_eq!(frame.layout_nodes, 0);
        assert_eq!(pixel(&p, 45, 100), CLEAR);
        assert_eq!(pixel(&p, 40, 100), WHITE);
        set_outline(&mut scene, id, None);
        let (_, p) = render(&mut gpu, &mut scene);
        assert_eq!(pixel(&p, 40, 100), CLEAR);
    }
}

#[test]
fn centred_and_inset_strokes_draw_over_content() {
    let _gpu_fixture = gpu_fixture();
    for (direct, mut gpu) in renderers() {
        // A centred stroke 4 wide: offset -2, the ring from 2 inside the
        // edge to 2 outside, over a red child filling the box.
        let mut scene = scene();
        let id = outlined(&mut scene, 0., 4., -2.);
        scene.append(
            id,
            NodeKind::Rect(Color(255, 0, 0, 255)),
            Style {
                width: Some(100.),
                height: Some(100.),
                ..Default::default()
            },
        );
        let (_, p) = render(&mut gpu, &mut scene);
        assert_eq!(pixel(&p, 49, 100), WHITE, "direct {direct}");
        assert_eq!(pixel(&p, 51, 100), WHITE, "over the child");
        assert_eq!(pixel(&p, 53, 100), [255, 0, 0, 255]);
        assert_eq!(pixel(&p, 47, 100), CLEAR);
    }
}

#[test]
fn outlines_and_shadows_past_a_clip_repaint_through_layers() {
    let _gpu_fixture = gpu_fixture();
    for (direct, mut gpu) in renderers() {
        // A clipping box with a shadow and an outline, scaled (its own
        // layer), then blurred, then moved inside a scaled parent: each
        // frame matches a full repaint (checked in `render`), so what it
        // paints past its clip is damaged where it was and where it lands.
        let mut scene = scene();
        let id = outlined(&mut scene, 6., 3., 1.);
        let NodeKind::Panel { layout, quad } = scene.kind(id).clone() else {
            unreachable!()
        };
        let shadow = BoxShadow {
            color: Color(0, 0, 255, 255),
            offset: Transform { x: 0., y: 20. },
            blur_radius: 0.,
            spread: 4.,
        };
        scene.set_kind(
            id,
            NodeKind::Panel {
                layout,
                quad: QuadStyle {
                    shadow: Some(shadow),
                    ..quad
                },
            },
        );
        let (_, p) = render(&mut gpu, &mut scene);
        assert_eq!(pixel(&p, 100, 160), [0, 0, 255, 255], "direct {direct}");
        assert_eq!(pixel(&p, 47, 100), WHITE);
        let scaled = LocalTransform {
            scale: (0.8, 0.8),
            ..LocalTransform::IDENTITY
        };
        scene.set_local_transform(id, scaled);
        let (_, p) = render(&mut gpu, &mut scene);
        assert_eq!(pixel(&p, 47, 100), CLEAR);
        let wider = Outline {
            width: 6.,
            color: Color(255, 255, 255, 255),
            offset: 4.,
        };
        set_outline(&mut scene, id, Some(wider));
        render(&mut gpu, &mut scene);
        scene.set_filter_blur(id, 2.);
        render(&mut gpu, &mut scene);
        set_outline(&mut scene, id, None);
        render(&mut gpu, &mut scene);
        scene.set_filter_blur(id, 0.);
        scene.set_local_transform(id, LocalTransform::IDENTITY);
        set_outline(&mut scene, id, Some(wider));
        render(&mut gpu, &mut scene);
        // Inside a transformed parent, its paint moves with it.
        let root = scene.root();
        let parent = scene.append(
            root,
            NodeKind::Container(Layout::Overlay),
            Style {
                width: Some(200.),
                height: Some(200.),
                ..Default::default()
            },
        );
        scene.set_local_transform(parent, scaled);
        let inner = outlined_in(&mut scene, parent, 0., 3., 1.);
        render(&mut gpu, &mut scene);
        scene.set_transform(inner, Transform { x: 30., y: 40. });
        render(&mut gpu, &mut scene);
        set_outline(&mut scene, inner, None);
        render(&mut gpu, &mut scene);
    }
}
