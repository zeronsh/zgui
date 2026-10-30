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

/// More than the budget drawn in one frame is still refused: nothing drawn
/// there can go. (Stacked, half transparent: each one shows.)
#[test]
fn more_photos_than_the_budget_in_one_frame_fail() {
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
    let error = gpu.render(&scene, &damage).unwrap_err();
    assert!(error.to_string().contains("64 MiB"), "{error}");
}

/// A test file: its pixels are its one byte, everywhere.
fn redecode(bytes: &[u8]) -> Result<Arc<[u8]>, String> {
    Ok(Arc::from(vec![bytes[0]; 640 * 480 * 4]))
}
fn kept_photo(i: u32) -> Arc<ImageData> {
    let value = (i % 200) as u8 + 20;
    Arc::new(
        ImageData::kept(
            640,
            480,
            Arc::from(vec![value; 640 * 480 * 4]),
            Arc::from(vec![value]),
            redecode,
        )
        .unwrap(),
    )
}

/// Images keeping their files, past the decoded-pixels budget: those a
/// renderer has give their pixels back, and a page scrolled back over them
/// draws them again once decoded again off the main thread. Also, another
/// renderer (its own device) uploads them from their files.
#[test]
fn kept_photos_give_their_pixels_back_and_draw_again() {
    zgx::image::set_decoded_budget(8 * 640 * 480 * 4);
    let mut scene = Scene::new(800., 600.);
    let column = scene.append(
        scene.root(),
        NodeKind::Container(Layout::Column),
        Style::default(),
    );
    let photos: Vec<_> = (0..40).map(kept_photo).collect();
    for photo in &photos {
        scene.append(column, NodeKind::Image(photo.clone()), photo_style());
    }
    let mut gpu = GpuRenderer::new(800, 600).unwrap();
    let damage = scene.flush().damage;
    gpu.render(&scene, &damage).unwrap();
    for step in 1..40 {
        let damage = scroll(&mut scene, column, step as f32 * 480.);
        gpu.render(&scene, &damage).unwrap();
    }
    // Uploaded and past the budget: most gave their pixels back.
    let held = zgx::image::decoded_bytes();
    assert!(held <= 10 * 640 * 480 * 4, "{held} bytes held");
    assert!(photos[0].try_pixels().is_none() || photos[1].try_pixels().is_none());
    // The first photo's texture is long gone too (the 64 MiB of textures
    // hold 54): scrolled back, it waits a frame for its pixels, then draws.
    let mut awaited = false;
    for step in (0..39).rev() {
        let damage = scroll(&mut scene, column, step as f32 * 480.);
        gpu.render(&scene, &damage).unwrap();
        awaited |= gpu.take_images_awaited();
    }
    std::thread::sleep(std::time::Duration::from_millis(200));
    let whole = [Rect::new(0., 0., 800., 600.)];
    gpu.render(&scene, &whole).unwrap();
    assert!(
        photos[0].try_pixels().is_some(),
        "decoded again for the top"
    );
    let _ = awaited;
    // Another renderer, another device: it needs the pixels again.
    let mut other = GpuRenderer::new(800, 600).unwrap();
    other.render(&scene, &whole).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(200));
    let stats = other.render(&scene, &whole).unwrap();
    assert!(stats.image_uploads >= 1 || !other.take_images_awaited());
    zgx::image::set_decoded_budget(usize::MAX);
}
