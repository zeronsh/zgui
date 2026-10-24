use std::sync::Arc;
use zgx::{compose::prelude::*, image::ImageData, scene::NodeKind, widgets::Ui};
fn source(w: u32, h: u32) -> Arc<ImageData> {
    Arc::new(ImageData::new(w, h, [255, 0, 0, 255].repeat((w * h) as usize)).unwrap())
}
#[test]
fn all_modes_center_in_padded_content_and_equal_styles_are_idle() {
    let mut ui = Ui::new(400., 300.);
    let fit = ui.signal(ObjectFit::Fill);
    let read = fit.clone();
    let tick = ui.signal(0);
    let reevaluate = tick.clone();
    let pixels = source(120, 60);
    let view = ui.mount(
        image("Image", pixels.clone())
            .size(200., 140.)
            .p(20.)
            .reactive_style(move || {
                let _ = reevaluate.get();
                Styles::new().object_fit(read.get())
            }),
    );
    ui.prepare_frame();
    let viewport = ui.scene.borrow().children(view.node())[0];
    let bitmap = ui.scene.borrow().children(viewport)[0];
    for (mode, expected) in [
        (ObjectFit::Fill, (20., 20., 160., 100.)),
        (ObjectFit::Contain, (20., 30., 160., 80.)),
        (ObjectFit::Cover, (0., 20., 200., 100.)),
        (ObjectFit::None, (40., 40., 120., 60.)),
        (ObjectFit::ScaleDown, (40., 40., 120., 60.)),
    ] {
        fit.set(mode);
        ui.prepare_frame();
        let scene = ui.scene.borrow();
        let rect = scene.bounds(bitmap);
        assert_eq!((rect.x, rect.y, rect.width, rect.height), expected);
        assert!(scene.style(viewport).clip);
        let NodeKind::Image(actual) = scene.kind(bitmap) else {
            panic!("bitmap missing")
        };
        assert!(Arc::ptr_eq(actual, &pixels));
        drop(scene);
        ui.scene.borrow_mut().flush();
        tick.update(|value| *value += 1);
        ui.prepare_frame();
        let idle = ui.scene.borrow_mut().flush();
        assert_eq!(idle.layout_nodes, 0);
        assert!(idle.damage.is_empty());
    }
}
#[test]
fn intrinsic_source_resize_and_scale_down_preserve_owned_nodes() {
    let mut ui = Ui::new(400., 300.);
    let data = ui.signal(source(120, 60));
    let read = data.clone();
    let extent = ui.signal(200.);
    let size = extent.clone();
    let view = ui.mount(
        image_signal("Image", move || read.get())
            .h(140.)
            .p(20.)
            .object_fit(ObjectFit::ScaleDown)
            .reactive_style(move || Styles::new().w(size.get())),
    );
    ui.prepare_frame();
    let viewport = ui.scene.borrow().children(view.node())[0];
    let bitmap = ui.scene.borrow().children(viewport)[0];
    extent.set(100.);
    ui.prepare_frame();
    assert_eq!(ui.scene.borrow().bounds(bitmap).width, 60.);
    assert_eq!(ui.scene.borrow().bounds(bitmap).height, 30.);
    data.set(source(30, 60));
    ui.prepare_frame();
    assert_eq!(ui.scene.borrow().bounds(bitmap).width, 30.);
    assert_eq!(ui.scene.borrow().bounds(bitmap).height, 60.);
    extent.set(20.);
    ui.prepare_frame();
    assert_eq!(ui.scene.borrow().bounds(bitmap).width, 0.);
    assert_eq!(ui.scene.borrow().bounds(bitmap).height, 0.);
    assert_eq!(ui.scene.borrow().children(viewport), &[bitmap]);
    view.unmount();
    assert!(!ui.scene.borrow().contains(bitmap));
}
#[test]
fn fit_is_sparse_noninherited_and_wrapper_overrides_inner_mode() {
    let mut ui = Ui::new(400., 300.);
    let pixels = source(120, 60);
    let mode = ui.signal(true);
    let read = mode.clone();
    let view = ui.mount(
        column().object_cover().children([
            image("Default", pixels.clone())
                .size(160., 100.)
                .id("default"),
            component(move |_| image("Refined", pixels).object_cover())
                .size(160., 100.)
                .object_contain()
                .id("refined")
                .reactive_style(move || {
                    if read.get() {
                        Styles::new().object_fit(ObjectFit::None)
                    } else {
                        Styles::new()
                    }
                }),
        ]),
    );
    ui.prepare_frame();
    let bitmap = |id| {
        let scene = ui.scene.borrow();
        scene.children(scene.children(view.find(id).unwrap())[0])[0]
    };
    let default = bitmap("default");
    let refined = bitmap("refined");
    assert_eq!(ui.scene.borrow().bounds(default).height, 100.);
    assert_eq!(ui.scene.borrow().bounds(refined).height, 60.);
    mode.set(false);
    ui.prepare_frame();
    assert_eq!(ui.scene.borrow().bounds(refined).height, 80.);
}

#[test]
fn fixed_fill_source_aspect_change_repaints_without_relayout_but_intrinsic_and_fit_reflow() {
    let mut ui = Ui::new(500., 400.);
    let source_signal = ui.signal(source(120, 60));
    let read = source_signal.clone();
    let fit = ui.signal(ObjectFit::Fill);
    let fit_read = fit.clone();
    let fixed = ui.mount(
        image_signal("Fixed", move || read.get())
            .size(200., 140.)
            .p(20.)
            .reactive_style(move || Styles::new().object_fit(fit_read.get())),
    );
    ui.prepare_frame();
    ui.scene.borrow_mut().flush();
    source_signal.set(source(60, 120));
    ui.prepare_frame();
    let changed = ui.scene.borrow_mut().flush();
    assert_eq!(
        changed.layout_nodes, 0,
        "explicit Fill geometry ignores source dimensions"
    );
    assert!(!changed.damage.is_empty());
    fit.set(ObjectFit::Contain);
    ui.prepare_frame();
    assert!(ui.scene.borrow_mut().flush().layout_nodes > 0);
    let viewport = ui.scene.borrow().children(fixed.node())[0];
    let bitmap = ui.scene.borrow().children(viewport)[0];
    assert_eq!(ui.scene.borrow().bounds(bitmap).width, 50.);
    source_signal.set(source(120, 60));
    ui.prepare_frame();
    assert!(ui.scene.borrow_mut().flush().layout_nodes > 0);
    assert_eq!(ui.scene.borrow().bounds(bitmap).width, 160.);
    fixed.unmount();
    let read = source_signal.clone();
    let intrinsic = ui.mount(image_signal("Intrinsic", move || read.get()));
    ui.prepare_frame();
    ui.scene.borrow_mut().flush();
    source_signal.set(source(30, 50));
    ui.prepare_frame();
    assert!(ui.scene.borrow_mut().flush().layout_nodes > 0);
    let bounds = ui.scene.borrow().bounds(intrinsic.node());
    assert_eq!((bounds.width, bounds.height), (30., 50.));
}
