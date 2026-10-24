//! Background images (`Background::Image`): a box's image fill placed as
//! its fit has it (fill, cover, contain), over its fill colour, cut to its
//! rounded corners, under its border and children; on the wgpu path and
//! with Metal directly.
use std::sync::Arc;
use zgx::decoration::{Background, Corners, Decoration};
use zgx::image::ImageData;
use zgx::scene::*;
use zgx::style::ObjectFit;
use zgx_gpu::GpuRenderer;

fn gpu_fixture() -> std::sync::MutexGuard<'static, ()> {
    static DEVICE_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());
    DEVICE_TEST
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
const SIZE: u32 = 200;
const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const WHITE: [u8; 4] = [255; 4];
const GREY: Color = Color(50, 50, 50, 255);
fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let at = ((y * SIZE + x) * 4) as usize;
    pixels[at..at + 4].try_into().unwrap()
}
/// An image of `columns` × `rows` texels from `texels`, row by row.
fn image(columns: u32, rows: u32, texels: &[[u8; 4]]) -> Arc<ImageData> {
    Arc::new(ImageData::new(columns, rows, texels.concat()).unwrap())
}
/// A box at (40, 40) of `size`, filled grey under `image` placed by `fit`.
fn boxed(
    scene: &mut Scene,
    size: (f32, f32),
    image: Arc<ImageData>,
    fit: ObjectFit,
    corners: Corners,
    border: f32,
) -> NodeId {
    let quad = QuadStyle {
        fill: GREY,
        border_color: Color(255, 255, 255, 255),
        border_width: border,
        decoration: Some(Arc::new(Decoration {
            background: Some(Background::Image { image, fit }),
            corners: Some(corners),
            ..Default::default()
        })),
        ..Default::default()
    };
    let root = scene.root();
    let id = scene.append(
        root,
        NodeKind::Panel {
            layout: Layout::Overlay,
            quad,
        },
        Style {
            width: Some(size.0),
            height: Some(size.1),
            ..Default::default()
        },
    );
    scene.set_transform(id, Transform { x: 40., y: 40. });
    id
}
fn scene() -> Scene {
    let mut scene = Scene::new(SIZE as f32, SIZE as f32);
    scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
    scene
}
/// Render the flushed frame, then check it against a full repaint.
fn render(gpu: &mut GpuRenderer, scene: &mut Scene) -> Vec<u8> {
    let damage = scene.flush().damage;
    gpu.render(scene, &damage).unwrap();
    let pixels = gpu.readback().unwrap();
    gpu.render(scene, &[Rect::new(0., 0., SIZE as f32, SIZE as f32)])
        .unwrap();
    assert_eq!(
        pixels,
        gpu.readback().unwrap(),
        "incremental frame differs from full"
    );
    pixels
}
fn renderers() -> impl Iterator<Item = (bool, GpuRenderer)> {
    [true, false].into_iter().map(|direct| {
        let mut gpu = GpuRenderer::new(SIZE, SIZE).unwrap();
        gpu.set_direct_metal(direct);
        (direct, gpu)
    })
}

#[test]
fn background_images_fill_cover_and_contain_their_box() {
    let _gpu_fixture = gpu_fixture();
    // Four columns: red, red, blue, blue; two rows alike.
    let stripes = image(4, 2, &[RED, RED, BLUE, BLUE, RED, RED, BLUE, BLUE]);
    let quarters = image(2, 2, &[RED, GREEN, BLUE, WHITE]);
    for (direct, mut gpu) in renderers() {
        // Fill: stretched to the box, a quarter each.
        let mut scene = scene();
        boxed(
            &mut scene,
            (100., 60.),
            quarters.clone(),
            ObjectFit::Fill,
            Corners::all(0.),
            0.,
        );
        let p = render(&mut gpu, &mut scene);
        assert_eq!(pixel(&p, 50, 50), RED, "direct {direct}");
        assert_eq!(pixel(&p, 130, 50), GREEN);
        assert_eq!(pixel(&p, 50, 90), BLUE);
        assert_eq!(pixel(&p, 130, 90), WHITE);
        // Cover: 200 × 100 centred on a 100 square, its middle columns.
        let mut scene = self::scene();
        boxed(
            &mut scene,
            (100., 100.),
            stripes.clone(),
            ObjectFit::Cover,
            Corners::all(0.),
            0.,
        );
        let p = render(&mut gpu, &mut scene);
        assert_eq!(pixel(&p, 60, 90), RED);
        assert_eq!(pixel(&p, 120, 90), BLUE);
        assert_eq!(pixel(&p, 42, 42), RED);
        // Contain: 100 × 50 centred, the grey fill above and below.
        let mut scene = self::scene();
        boxed(
            &mut scene,
            (100., 100.),
            stripes.clone(),
            ObjectFit::Contain,
            Corners::all(0.),
            0.,
        );
        let p = render(&mut gpu, &mut scene);
        assert_eq!(pixel(&p, 50, 50), [50, 50, 50, 255]);
        assert_eq!(pixel(&p, 50, 90), RED);
        assert_eq!(pixel(&p, 130, 90), BLUE);
        assert_eq!(pixel(&p, 50, 130), [50, 50, 50, 255]);
    }
}

#[test]
fn background_images_round_with_their_box_under_border_and_children() {
    let _gpu_fixture = gpu_fixture();
    let red = image(1, 1, &[RED]);
    for (direct, mut gpu) in renderers() {
        let mut scene = scene();
        let id = boxed(
            &mut scene,
            (100., 100.),
            red.clone(),
            ObjectFit::Cover,
            Corners::all(20.),
            3.,
        );
        let p = render(&mut gpu, &mut scene);
        // Cut to the corner's curve; the border over the image.
        assert_eq!(pixel(&p, 42, 42), [0; 4], "direct {direct}");
        assert_eq!(pixel(&p, 90, 41), WHITE);
        assert_eq!(pixel(&p, 90, 90), RED);
        // A child paints over it.
        let child = scene.append(
            id,
            NodeKind::Rect(Color(0, 0, 255, 255)),
            Style {
                width: Some(20.),
                height: Some(20.),
                ..Default::default()
            },
        );
        scene.set_transform(child, Transform { x: 40., y: 40. });
        let p = render(&mut gpu, &mut scene);
        assert_eq!(pixel(&p, 90, 90), BLUE);
        assert_eq!(pixel(&p, 70, 70), RED);
        // Resized, the image follows the box.
        let mut style = scene.style(id);
        style.width = Some(140.);
        scene.set_style(id, style);
        let p = render(&mut gpu, &mut scene);
        assert_eq!(pixel(&p, 170, 90), RED);
        assert_eq!(pixel(&p, 178, 42), [0; 4]);
    }
}
