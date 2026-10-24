use std::sync::Arc;
use zgx::{compose::prelude::*, image::ImageData, scene::NodeKind, semantics::Role, widgets::Ui};
fn source(w: u32, h: u32, red: u8) -> Arc<ImageData> {
    Arc::new(ImageData::new(w, h, [red, 0, 0, 255].repeat((w * h) as usize)).unwrap())
}
#[test]
fn image_intrinsic_size_follows_source_and_preserves_nodes() {
    let mut ui = Ui::new(600., 300.);
    let data = ui.signal(source(80, 40, 255));
    let read = data.clone();
    let view = ui.mount(image_signal("Landscape", move || read.get()).p(5.));
    let root = view.node();
    ui.prepare_frame();
    let viewport = ui.scene.borrow().children(root)[0];
    let bitmap = ui.scene.borrow().children(viewport)[0];
    assert_eq!(ui.scene.borrow().bounds(root).width, 90.);
    assert_eq!(ui.scene.borrow().bounds(root).height, 50.);
    for (w, h) in [(120, 60), (30, 20), (100, 50)] {
        data.set(source(w, h, 100));
        ui.prepare_frame();
        let scene = ui.scene.borrow();
        assert_eq!(scene.children(root), &[viewport]);
        assert_eq!(scene.children(viewport), &[bitmap]);
        assert_eq!(
            (scene.bounds(root).width, scene.bounds(root).height),
            (w as f32 + 10., h as f32 + 10.)
        );
        assert_eq!(
            (scene.bounds(bitmap).width, scene.bounds(bitmap).height),
            (w as f32, h as f32)
        );
    }
    let sem = ui.semantics.borrow().get(root).unwrap().clone();
    assert_eq!(sem.role, Role::Image);
    assert_eq!(sem.label, "Landscape");
    assert!(ui.semantics.borrow().get(bitmap).is_none());
}
#[test]
fn image_fills_allocated_content_and_reacts_to_padding_and_constraints() {
    let mut ui = Ui::new(600., 300.);
    let wide = ui.signal(false);
    let read = wide.clone();
    let view = ui.mount(
        image("Icon", source(80, 40, 255))
            .w(200.)
            .h(100.)
            .max_w(180.)
            .bg(rgb(0x123456))
            .overflow_hidden()
            .reactive_style(move || {
                if read.get() {
                    Styles::new().p(20.).max_w(150.)
                } else {
                    Styles::new().p(10.)
                }
            }),
    );
    ui.prepare_frame();
    let root = view.node();
    let viewport = ui.scene.borrow().children(root)[0];
    let bitmap = ui.scene.borrow().children(viewport)[0];
    assert_eq!(
        (
            ui.scene.borrow().bounds(bitmap).width,
            ui.scene.borrow().bounds(bitmap).height
        ),
        (160., 80.)
    );
    assert!(matches!(
        ui.scene.borrow().kind(root),
        NodeKind::Panel { .. }
    ));
    assert!(ui.scene.borrow().style(root).clip);
    wide.set(true);
    ui.prepare_frame();
    let bounds = ui.scene.borrow().bounds(bitmap);
    assert_eq!(
        (bounds.x, bounds.y, bounds.width, bounds.height),
        (20., 20., 110., 60.)
    );
}
#[test]
fn image_same_source_is_idle_and_unmount_drops_binding_and_pixels() {
    let mut ui = Ui::new(300., 200.);
    let pixels = source(80, 40, 255);
    let weak = Arc::downgrade(&pixels);
    let data = ui.signal(pixels.clone());
    let read = data.clone();
    let view = ui.mount(image_signal("Sample", move || read.get()));
    ui.prepare_frame();
    ui.scene.borrow_mut().flush();
    data.set(pixels.clone());
    ui.prepare_frame();
    assert!(ui.scene.borrow_mut().flush().damage.is_empty());
    let viewport = ui.scene.borrow().children(view.node())[0];
    let bitmap = ui.scene.borrow().children(viewport)[0];
    view.unmount();
    assert!(ui.semantics.borrow().get(view.node()).is_none());
    assert!(!ui.scene.borrow().contains(bitmap));
    data.set(source(1, 1, 0));
    drop(pixels);
    assert!(weak.upgrade().is_none());
}
#[test]
fn image_rejects_children_without_leaking_partial_mount() {
    let mut ui = Ui::new(300., 200.);
    let prior = ui.mount(text("existing"));
    let root = prior.node();
    let pixels = source(1, 1, 0);
    let weak = Arc::downgrade(&pixels);
    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ui.mount(
            column()
                .child(text("staged"))
                .child(image("Bad", pixels).child(text("invalid"))),
        );
    }));
    assert!(failed.is_err());
    assert!(weak.upgrade().is_none());
    assert!(prior.is_mounted());
    assert_eq!(ui.scene.borrow().children(ui.root()), &[root]);
}
