//! Images' own textures past their budget (64 MiB): those not drawn this
//! frame go, least recently drawn first, and upload again when drawn. A long
//! page of photos scrolled through drew until its 54th, then every frame
//! failed.
use std::sync::Arc;
use zgx::{image::ImageData, scene::*};
use zgx_gpu::GpuRenderer;

fn photo(i: u32) -> NodeKind {
    let pixels: Vec<u8> = (0..640 * 480)
        .flat_map(|p| [(p % 251) as u8, (i * 2) as u8, 90, 255])
        .collect();
    NodeKind::Image(Arc::new(ImageData::new(640, 480, pixels).unwrap()))
}
fn photo_style() -> Style {
    Style {
        width: Some(640.),
        height: Some(480.),
        ..Default::default()
    }
}
fn page(height: f32, photos: u32) -> (Scene, NodeId) {
    let mut scene = Scene::new(800., height);
    let column = scene.append(
        scene.root(),
        NodeKind::Container(Layout::Column),
        Style::default(),
    );
    for i in 0..photos {
        scene.append(column, photo(i), photo_style());
    }
    (scene, column)
}
fn scroll(scene: &mut Scene, column: NodeId, to: f32) -> Vec<Rect> {
    scene.set_style(
        column,
        Style {
            margin: Insets {
                top: -to,
                ..Default::default()
            },
            ..Default::default()
        },
    );
    scene.flush().damage
}

#[test]
fn a_long_page_of_photos_scrolls_down_and_back() {
    let (mut scene, column) = page(600., 100);
    let mut gpu = GpuRenderer::new(800, 600).unwrap();
    let damage = scene.flush().damage;
    gpu.render(&scene, &damage).unwrap();
    // 100 photos of 1.2 MB: the budget holds about 54.
    for step in 1..100 {
        let damage = scroll(&mut scene, column, step as f32 * 480.);
        gpu.render(&scene, &damage)
            .unwrap_or_else(|e| panic!("photo {step}: {e}"));
    }
    // Back to the top: the photos given back upload again as they show.
    let mut uploads = 0;
    for step in (0..99).rev() {
        let damage = scroll(&mut scene, column, step as f32 * 480.);
        uploads += gpu.render(&scene, &damage).unwrap().image_uploads;
    }
    assert!(
        uploads > 0 && uploads < 99,
        "{uploads} uploads scrolling back"
    );
}

/// More than the budget drawn in one frame: nothing drawn there can go, so
/// the last are drawn at a lower resolution, once said, and none is left
/// out. (Stacked, half transparent: each one shows.)
#[test]
fn more_photos_than_the_budget_in_one_frame_draw_what_fits() {
    let mut scene = Scene::new(800., 600.);
    let stack = scene.append(
        scene.root(),
        NodeKind::Container(Layout::Overlay),
        Style::default(),
    );
    for i in 0..60u32 {
        let pixels: Vec<u8> = (0..640 * 480)
            .flat_map(|p| [(p % 251) as u8, (i * 2) as u8, 90, 128])
            .collect();
        let image = Arc::new(ImageData::new(640, 480, pixels).unwrap());
        scene.append(stack, NodeKind::Image(image), photo_style());
    }
    let mut gpu = GpuRenderer::new(800, 600).unwrap();
    let damage = scene.flush().damage;
    gpu.render(&scene, &damage).unwrap();
    let issues = gpu.take_issues();
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert!(issues[0].contains("lower resolution"), "{issues:?}");
    let stats = gpu.debug_cache_stats();
    assert_eq!(stats.image_textures, 60, "every one drawn");
    assert!(
        stats.image_bytes <= 64 * 1024 * 1024,
        "{}",
        stats.image_bytes
    );
}

/// A script's picture keeps its file: nothing is decoded until it is
/// drawn, the first frame goes without it (and says it waited), and once
/// decoded off the main thread it draws at the size it is shown, its
/// decoded copy ready to go.
#[test]
fn a_kept_file_draws_once_decoded_at_the_size_it_shows() {
    let mut png = Vec::new();
    image::RgbaImage::from_pixel(400, 300, image::Rgba([230, 20, 20, 255]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let picture = zgx_gpu::assets::decode_image_kept(Arc::from(png)).unwrap();
    assert!(picture.keeps_file());
    assert_eq!((picture.width(), picture.height()), (400, 300));
    let mut scene = Scene::new(200., 200.);
    scene.append(
        scene.root(),
        NodeKind::Image(picture.clone()),
        Style {
            width: Some(100.),
            height: Some(75.),
            ..Default::default()
        },
    );
    let mut gpu = GpuRenderer::new(200, 200).unwrap();
    let damage = scene.flush().damage;
    gpu.render(&scene, &damage).unwrap();
    // (`images_decoded` takes that the first frame waited for it.)
    assert_eq!(gpu.debug_cache_stats().image_textures, 0);
    let mut drawn = false;
    for _ in 0..500 {
        std::thread::sleep(std::time::Duration::from_millis(4));
        // As the host does once told they are decoded: not otherwise, so a
        // frame that patched only what changed would be caught.
        if gpu.images_decoded() {
            gpu.render(&scene, &[Rect::new(0., 0., 200., 200.)])
                .unwrap();
        }
        if gpu.debug_cache_stats().image_textures == 1 {
            drawn = true;
            break;
        }
    }
    assert!(drawn, "drawn once decoded");
    assert_eq!(
        gpu.debug_cache_stats().image_bytes,
        100 * 75 * 4,
        "at the size it shows"
    );
    assert!(gpu.take_images_uploaded(), "its decoded copy may go");
    assert!(gpu.readback().unwrap()[(30 * 200 + 30) * 4] > 200);
}
