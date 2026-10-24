use std::{cell::Cell, rc::Rc};
use zgx::{compose::prelude::*, widgets::Ui};

#[test]
fn percentages_use_parent_content_box_and_reactive_resize_retains_children() {
    let mut ui = Ui::new(600., 500.);
    let size = ui.signal((240., 200.));
    let read = size.clone();
    let builds = Rc::new(Cell::new(0));
    let count = builds.clone();
    let mounted = ui.mount(
        column()
            .p(20.)
            .reactive_style(move || {
                let (width, height) = read.get();
                Styles::new().size(width, height)
            })
            .child(component(move |_| {
                count.set(count.get() + 1);
                column().w_percent(50.).h_percent(50.).id("child")
            })),
    );
    ui.prepare_frame();
    let child = mounted.find("child").unwrap();
    let first = ui.scene.borrow().bounds(child);
    assert_eq!(
        (first.x, first.y, first.width, first.height),
        (20., 20., 100., 80.)
    );
    size.set((400., 300.));
    ui.prepare_frame();
    let next = ui.scene.borrow().bounds(child);
    assert_eq!((next.width, next.height), (180., 130.));
    assert_eq!(mounted.find("child"), Some(child));
    assert_eq!(builds.get(), 1);
}

#[test]
fn sparse_patches_override_pixels_and_percentages_and_can_restore_base() {
    let mut ui = Ui::new(500., 400.);
    let mode = ui.signal(0);
    let read = mode.clone();
    let mounted = ui.mount(
        column().size(300., 200.).children([
            column().w(30.).w_percent(50.).h(10.).id("percent_last"),
            column().w_percent(50.).w(30.).h(10.).id("pixels_last"),
            component(|_| column().w(9.).h(9.))
                .w_full()
                .h(10.)
                .id("wrapper"),
            column()
                .size(30., 20.)
                .reactive_style(move || match read.get() {
                    0 => Styles::new().w_percent(50.).h_percent(25.),
                    1 => Styles::new().size(70., 40.),
                    _ => Styles::new(),
                })
                .id("dynamic"),
        ]),
    );
    ui.prepare_frame();
    for (id, width) in [
        ("percent_last", 150.),
        ("pixels_last", 30.),
        ("wrapper", 300.),
    ] {
        assert_eq!(
            ui.scene.borrow().bounds(mounted.find(id).unwrap()).width,
            width
        );
    }
    let dynamic = mounted.find("dynamic").unwrap();
    let first = ui.scene.borrow().bounds(dynamic);
    assert_eq!((first.width, first.height), (150., 50.));
    mode.set(1);
    ui.prepare_frame();
    let pixels = ui.scene.borrow().bounds(dynamic);
    assert_eq!((pixels.width, pixels.height), (70., 40.));
    mode.set(2);
    ui.prepare_frame();
    let base = ui.scene.borrow().bounds(dynamic);
    assert_eq!((base.width, base.height), (30., 20.));
}

#[test]
fn full_size_tracks_window_and_equal_style_patches_produce_no_layout_or_damage() {
    let mut ui = Ui::new(300., 200.);
    let epoch = ui.signal(0);
    let read = epoch.clone();
    let mounted = ui.mount(column().w_full().h_full().p(10.).child(
        column().id("child").reactive_style(move || {
            let _ = read.get();
            Styles::new().w_full().h_full()
        }),
    ));
    ui.prepare_frame();
    let child = mounted.find("child").unwrap();
    assert_eq!(ui.scene.borrow().bounds(child).width, 280.);
    assert_eq!(ui.scene.borrow().bounds(child).height, 180.);
    ui.scene.borrow_mut().resize(450., 350.);
    ui.prepare_frame();
    assert_eq!(ui.scene.borrow().bounds(child).width, 430.);
    assert_eq!(ui.scene.borrow().bounds(child).height, 330.);
    ui.scene.borrow_mut().flush();
    epoch.set(1);
    ui.prepare_frame();
    let idle = ui.scene.borrow_mut().flush();
    assert_eq!(idle.layout_nodes, 0);
    assert!(idle.damage.is_empty());
}

#[test]
fn percentage_editor_reflows_caret_after_parent_resize_without_editing_model() {
    let mut ui = Ui::new(500., 400.);
    let width = ui.signal(220.);
    let read = width.clone();
    let value = ui.signal("abcdefghijklmnopqrstuvwxyz".to_owned());
    let mounted = ui.mount(
        column()
            .h(200.)
            .p(10.)
            .reactive_style(move || Styles::new().w(read.get()))
            .child(
                text_area("Editor", value.clone())
                    .id("editor")
                    .w_full()
                    .h(150.)
                    .p(5.)
                    .text_size(10.)
                    .line_height(20.)
                    .text_wrap(true),
            ),
    );
    ui.prepare_frame();
    let node = mounted.find("editor").unwrap();
    ui.input.focus(&ui.scene, Some(node));
    let editor = ui.focused_editor().unwrap();
    editor.editor.borrow_mut().set_selection(24, 24);
    editor.refresh();
    ui.prepare_frame();
    let first = ui.scene.borrow().bounds(editor.caret);
    assert_eq!(ui.scene.borrow().bounds(node).width, 200.);
    width.set(100.);
    ui.prepare_frame();
    let next = ui.scene.borrow().bounds(editor.caret);
    let bounds = ui.scene.borrow().bounds(node);
    assert_eq!(bounds.width, 80.);
    assert!(next.y > first.y, "caret follows new wrapped visual line");
    assert!(next.x >= bounds.x && next.x < bounds.x + bounds.width);
    assert_eq!(editor.editor.borrow().selection().focus, 24);
    assert_eq!(value.get(), "abcdefghijklmnopqrstuvwxyz");
}

#[test]
fn percentage_image_overrides_intrinsic_size_and_stretches_after_parent_resize() {
    use std::sync::Arc;
    use zgx::image::ImageData;
    let mut ui = Ui::new(500., 400.);
    let width = ui.signal(200.);
    let read = width.clone();
    let pixels = Arc::new(ImageData::new(8, 4, [255, 0, 0, 255].repeat(32)).unwrap());
    let mounted = ui.mount(
        column()
            .h(160.)
            .p(10.)
            .reactive_style(move || Styles::new().w(read.get()))
            .child(
                image("Image", pixels)
                    .w_percent(50.)
                    .h_percent(50.)
                    .p(5.)
                    .id("image"),
            ),
    );
    ui.prepare_frame();
    let node = mounted.find("image").unwrap();
    let viewport = ui.scene.borrow().children(node)[0];
    let bitmap = ui.scene.borrow().children(viewport)[0];
    let outer = ui.scene.borrow().bounds(node);
    let inner = ui.scene.borrow().bounds(bitmap);
    assert_eq!((outer.width, outer.height), (90., 70.));
    assert_eq!((inner.width, inner.height), (80., 60.));
    width.set(300.);
    ui.prepare_frame();
    let inner = ui.scene.borrow().bounds(bitmap);
    assert_eq!((inner.width, inner.height), (130., 60.));
    assert_eq!(ui.scene.borrow().children(node), &[viewport]);
    assert_eq!(ui.scene.borrow().children(viewport), &[bitmap]);
}

#[test]
fn growing_split_retains_full_height_siblings_through_editor_refresh_and_window_resize() {
    use zgx::input::InputEvent;
    let mut ui = Ui::new(640., 420.);
    let value = ui.signal("Resize this window. Both panels keep half the available width, and this editor wraps without losing its text or selection.".to_owned());
    let mounted = ui.mount(
        column()
            .w_full()
            .h_full()
            .p(20.)
            .gap(12.)
            .text_size(16.)
            .child(button().size(120., 24.).child(text("Report")))
            .child(
                row()
                    .id("split")
                    .w_full()
                    .grow()
                    .flex_shrink(1.)
                    .child(
                        text_area("Resizable editor", value.clone())
                            .id("editor")
                            .w_percent(50.)
                            .h_full()
                            .p(12.)
                            .text_wrap(true)
                            .bg(rgb(0x203050)),
                    )
                    .child(
                        column()
                            .id("right")
                            .w_percent(50.)
                            .h_full()
                            .p(12.)
                            .bg(rgb(0x283b32))
                            .child(text("50% width").text_size(20.))
                            .child(
                                text("Resize the window; the editor stays mounted.")
                                    .text_wrap(true),
                            ),
                    ),
            ),
    );
    // Native hosts focus immediately after mounting, before their first draw.
    ui.input.focus(&ui.scene, mounted.find("editor"));
    let editor = ui.focused_editor().unwrap();
    let right = mounted.find("right").unwrap();
    let split = mounted.find("split").unwrap();
    for (width, height) in [(640., 420.), (800., 500.), (420., 400.), (640., 420.)] {
        ui.scene.borrow_mut().resize(width, height);
        for refresh in 0..3 {
            if refresh == 1 {
                ui.dispatch(InputEvent::Text("!".into()));
            }
            editor.refresh();
            ui.prepare_frame();
            let scene = ui.scene.borrow();
            let allocated = scene.bounds(split);
            assert_eq!(allocated.height, height - 76.);
            assert_eq!(
                scene.bounds(editor.node).height,
                allocated.height,
                "editor refresh {refresh}"
            );
            assert_eq!(
                scene.bounds(right).height,
                allocated.height,
                "right refresh {refresh} at {width}x{height}"
            );
            assert_eq!(scene.bounds(right).width, (width - 40.) / 2.);
        }
    }
    assert_eq!(mounted.find("editor"), Some(editor.node));
    assert!(value.get().contains('!'));
}
