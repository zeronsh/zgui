//! A lost GPU device (a driver reset, a removed GPU) fails a renderer's
//! frames instead of panicking, and a renderer on a new context draws.
use zgui::scene::{Color, Layout, NodeKind, Rect, Scene, Style};
use zgui_gpu::{GpuContext, GpuRenderer};

fn scene() -> Scene {
    let mut scene = Scene::new(64., 64.);
    scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
    let root = scene.root();
    scene.append(
        root,
        NodeKind::Rect(Color(255, 0, 0, 255)),
        Style {
            width: Some(32.),
            height: Some(32.),
            ..Default::default()
        },
    );
    scene
}

#[test]
fn a_lost_device_fails_frames_and_a_new_context_draws() {
    let Ok(context) = GpuContext::new() else {
        eprintln!("no GPU here");
        return;
    };
    let mut gpu = GpuRenderer::new_with_context(64, 64, &context).unwrap();
    let mut scene = scene();
    let full = [Rect::new(0., 0., 64., 64.)];
    scene.flush();
    gpu.render(&scene, &full).unwrap();
    assert_eq!(context.lost(), None);
    context.debug_lose_device();
    assert!(context.lost().is_some());
    let error = gpu.render(&scene, &full).unwrap_err();
    assert!(error.is_device_lost(), "{error}");
    assert!(gpu.present().unwrap_err().is_device_lost());
    assert!(gpu.readback().unwrap_err().is_device_lost());
    // A new context: the frame draws again.
    let mut gpu = GpuRenderer::new(64, 64).unwrap();
    gpu.render(&scene, &full).unwrap();
    let pixels = gpu.readback().unwrap();
    assert_eq!(&pixels[..4], &[255, 0, 0, 255]);
}

/// A context made on another thread (`prepare_in_background`) draws as one
/// made on this thread does, and fails as it would where there is no GPU.
#[test]
fn a_context_made_in_the_background_draws_like_one_made_here() {
    let pending = GpuContext::prepare_in_background();
    let here = GpuContext::new();
    let background = pending.join();
    assert_eq!(
        here.is_ok(),
        background.is_ok(),
        "both find a GPU, or neither"
    );
    let (Ok(here), Ok(background)) = (here, background) else {
        eprintln!("no GPU here");
        return;
    };
    let mut scene = scene();
    scene.flush();
    let full = [Rect::new(0., 0., 64., 64.)];
    let mut pixels = Vec::new();
    for context in [&here, &background] {
        let mut gpu = GpuRenderer::new_with_context(64, 64, context).unwrap();
        gpu.render(&scene, &full).unwrap();
        pixels.push(gpu.readback().unwrap());
    }
    assert_eq!(pixels[0], pixels[1]);
}
