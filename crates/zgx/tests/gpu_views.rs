//! GPU views (`compose::gpu_view`): a frame is an image of its own identity
//! and no pixels, made as the box shows, resizes, is invalidated or animates,
//! and nothing else; showing it repaints its box alone, with the scene's
//! resources counted unchanged.
use std::{cell::RefCell, rc::Rc, sync::Arc, time::Instant};
use zgx::{
    compose::{TaskRunner, prelude::*},
    frame::{Frame, FrameClock},
    gpu_view::GpuView,
    image::ImageData,
    reactive::Signal,
    scene::{NodeId, NodeKind, Rect, Scene},
    task::LocalExecutor,
    widgets::Ui,
};

/// The node a GPU view's box shows its frames in.
fn frame_node(scene: &Scene, root: NodeId) -> NodeId {
    scene.children(scene.children(root)[0])[0]
}
fn frame(scene: &Scene, root: NodeId) -> Option<Arc<ImageData>> {
    match scene.kind(frame_node(scene, root)) {
        NodeKind::Image(image) => Some(image.clone()),
        _ => None,
    }
}
fn mounted(view: &GpuView) -> (Ui, NodeId, Signal<f32>) {
    let mut ui = Ui::new(200., 100.);
    let width = ui.signal(60.);
    let set = width.clone();
    let handle = ui.mount(
        div().size(200., 100.).child(
            gpu_view("Model", view)
                .id("view")
                .absolute()
                .ml(20.)
                .mt(10.)
                .h(40.)
                .reactive_style(move || Styles::new().w(width.get())),
        ),
    );
    ui.prepare_frame();
    let root = handle.find("view").unwrap();
    (ui, root, set)
}

#[test]
fn a_shown_view_has_a_frame_its_box_s_size_and_no_pixels() {
    let view = GpuView::new(());
    let (ui, root, _) = mounted(&view);
    let scene = ui.scene.borrow();
    let image = frame(&scene, root).expect("a frame once laid out");
    let gpu = image.gpu_frame().expect("a GPU view's frame");
    assert_eq!((gpu.view, gpu.width, gpu.height), (view.id(), 60., 40.));
    assert!(image.pixels().is_empty());
    assert_eq!(
        scene.bounds(frame_node(&scene, root)),
        Rect::new(20., 10., 60., 40.)
    );
}

#[test]
fn invalidating_repaints_its_box_alone_with_resources_unchanged() {
    let view = GpuView::new(());
    let (ui, root, _) = mounted(&view);
    ui.scene.borrow_mut().flush();
    let before = frame(&ui.scene.borrow(), root).unwrap();
    let revision = ui.scene.borrow().content_revision();
    view.invalidate();
    view.invalidate();
    let report = ui.scene.borrow_mut().flush();
    let after = frame(&ui.scene.borrow(), root).unwrap();
    assert_ne!(before.id(), after.id(), "a new frame");
    assert_eq!(ui.scene.borrow().content_revision(), revision);
    let damage = report.damage.iter().fold(None::<Rect>, |all, rect| {
        Some(all.map_or(*rect, |all| all.union(*rect)))
    });
    assert_eq!(damage, Some(Rect::new(20., 10., 60., 40.)));
    // Nothing asked, nothing changes.
    let report = ui.scene.borrow_mut().flush();
    assert!(report.damage.is_empty());
    assert_eq!(frame(&ui.scene.borrow(), root).unwrap().id(), after.id());
}

#[test]
fn frames_take_turns_in_two_images() {
    let view = GpuView::new(());
    let (ui, root, _) = mounted(&view);
    let mut seen = Vec::new();
    for _ in 0..6 {
        view.invalidate();
        ui.scene.borrow_mut().flush();
        let image = frame(&ui.scene.borrow(), root).unwrap();
        seen.push((Arc::as_ptr(&image), image.id()));
    }
    let images: std::collections::HashSet<_> = seen.iter().map(|(at, _)| *at).collect();
    assert_eq!(images.len(), 2, "made again in place: {seen:?}");
    let ids: std::collections::HashSet<_> = seen.iter().map(|(_, id)| *id).collect();
    assert_eq!(ids.len(), 6, "each frame of its own identity");
}

#[test]
fn a_resized_box_has_a_frame_its_new_size() {
    let view = GpuView::new(());
    let (ui, root, width) = mounted(&view);
    width.set(90.5);
    ui.prepare_frame();
    let image = frame(&ui.scene.borrow(), root).unwrap();
    let gpu = image.gpu_frame().unwrap();
    assert_eq!((gpu.width, gpu.height), (90.5, 40.));
    assert_eq!((image.width(), image.height()), (91, 40));
    let scene = ui.scene.borrow();
    assert_eq!(
        scene.bounds(frame_node(&scene, root)),
        Rect::new(20., 10., 90.5, 40.)
    );
}

#[test]
fn an_animating_view_asks_a_frame_each_refresh_while_it_shows() {
    let view = GpuView::new(());
    let mut ui = Ui::new(200., 100.);
    let executor = Rc::new(RefCell::new(LocalExecutor::new()));
    let clock = FrameClock::new();
    let handle = ui.mount(provide(
        clock.clone(),
        provide(
            TaskRunner::from_executor(executor.clone()),
            gpu_view("Model", &view).id("view").size(60., 40.),
        ),
    ));
    ui.prepare_frame();
    let root = handle.find("view").unwrap();
    executor.borrow_mut().tick();
    assert!(executor.borrow().is_empty(), "not animating: no timer");
    assert!(!clock.wants_frame());
    view.animate(true);
    executor.borrow_mut().tick();
    assert!(clock.wants_frame());
    let shown = || frame(&ui.scene.borrow(), root).unwrap().id();
    let mut last = shown();
    for index in 1..4 {
        clock.deliver(Frame {
            index,
            time: Instant::now(),
            interval: std::time::Duration::from_millis(8),
        });
        executor.borrow_mut().tick();
        assert_ne!(shown(), last, "a frame for refresh {index}");
        last = shown();
    }
    // Unpresented (a hidden window), it waits without a timer.
    ui.set_presented(false);
    executor.borrow_mut().tick();
    assert!(executor.borrow().is_empty());
    ui.set_presented(true);
    executor.borrow_mut().tick();
    assert!(!executor.borrow().is_empty());
    view.animate(false);
    executor.borrow_mut().tick();
    assert!(executor.borrow().is_empty());
    assert!(!clock.wants_frame());
    handle.unmount();
    // Gone, its box asks nothing.
    view.invalidate();
    view.animate(true);
    executor.borrow_mut().tick();
    assert!(executor.borrow().is_empty());
}

#[test]
fn a_frame_names_its_painter() {
    let view = GpuView::new(7_u32);
    assert_eq!(
        zgx::gpu_view::painter(view.id())
            .unwrap()
            .downcast_ref::<u32>(),
        Some(&7)
    );
    let id = view.id();
    drop(view);
    assert!(zgx::gpu_view::painter(id).is_none());
}
