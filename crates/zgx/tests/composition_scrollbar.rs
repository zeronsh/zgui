use zgx::{
    compose::prelude::*,
    input::{InputEvent, Key, Modifiers, PointerButton},
    scene::NodeId,
    semantics::Role,
    widgets::Ui,
};
fn bar(ui: &Ui) -> NodeId {
    ui.semantics
        .borrow()
        .iter()
        .find(|(_, n)| n.role == Role::ScrollBar)
        .unwrap()
        .0
}
fn down(ui: &mut Ui, x: f32, y: f32) {
    ui.dispatch(InputEvent::PointerDown {
        x,
        y,
        button: PointerButton::Primary,
    });
}
fn up(ui: &mut Ui, x: f32, y: f32) {
    ui.dispatch(InputEvent::PointerUp {
        x,
        y,
        button: PointerButton::Primary,
    });
}
#[test]
fn scrollbar_track_paging_thumb_capture_resize_and_cancel() {
    let mut ui = Ui::new(500., 400.);
    let offset = ui.signal(0.);
    let height = ui.signal(120.);
    let read = height.clone();
    ui.mount(
        scroll(offset.clone())
            .w(200.)
            .p(10.)
            .reactive_style(move || Styles::new().h(read.get()))
            .scrollbar(true)
            .child(div().h(400.)),
    );
    ui.prepare_frame();
    let bar = bar(&ui);
    let bounds = ui.scene.borrow().bounds(bar);
    assert_eq!(
        (bounds.x, bounds.y, bounds.width, bounds.height),
        (182., 10., 8., 100.)
    );
    down(&mut ui, 186., 90.);
    up(&mut ui, 186., 90.);
    assert_eq!(offset.get(), 100.);
    assert_eq!(ui.input.focused(), Some(bar));
    ui.dispatch(InputEvent::KeyDown {
        key: Key::End,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    assert_eq!(offset.get(), 300.);
    offset.set(100.);
    down(&mut ui, 186., 40.);
    assert_eq!(ui.input.captured(), Some(bar));
    height.set(220.);
    ui.prepare_frame();
    ui.dispatch(InputEvent::PointerMove { x: 400., y: 390. });
    assert_eq!(offset.get(), 200.);
    ui.dispatch(InputEvent::PointerCancel);
    assert_eq!(ui.input.captured(), None);
    ui.dispatch(InputEvent::PointerMove { x: 186., y: 20. });
    assert_eq!(offset.get(), 200.);
}
#[test]
fn horizontal_scrollbar_keyboard_numeric_and_translation_only() {
    let mut ui = Ui::new(500., 300.);
    let offset = ui.signal(0.);
    let view = ui.mount(
        scroll_x(offset.clone())
            .w(200.)
            .h(80.)
            .scrollbar(true)
            .child(div().w(800.)),
    );
    ui.prepare_frame();
    let bar = bar(&ui);
    ui.scene.borrow_mut().flush();
    assert!(ui.input.focus(&ui.scene, Some(bar)));
    for (key, expected) in [
        (Key::PageDown, 200.),
        (Key::ArrowRight, 240.),
        (Key::End, 600.),
        (Key::PageUp, 400.),
        (Key::Home, 0.),
    ] {
        ui.dispatch(InputEvent::KeyDown {
            key,
            modifiers: Modifiers::default(),
            repeat: false,
        });
        assert_eq!(offset.get(), expected);
        ui.prepare_frame();
        assert_eq!(ui.scene.borrow_mut().flush().layout_nodes, 0);
    }
    ui.input
        .dispatch_to(&ui.scene, bar, InputEvent::SetNumericValue(125.));
    assert_eq!(offset.get(), 125.);
    assert_eq!(
        ui.semantics.borrow().get(bar).unwrap().numeric_value,
        Some(125.)
    );
    view.unmount();
    assert!(!ui.input.has_listeners(bar));
}
#[test]
fn hidden_scrollbar_releases_focus_and_allows_underlying_button() {
    let mut ui = Ui::new(500., 300.);
    let offset = ui.signal(0.);
    let tall = ui.signal(true);
    let read = tall.clone();
    let calls = ui.signal(0);
    let click = calls.clone();
    let view = ui.mount(
        scroll(offset.clone())
            .w(200.)
            .h(100.)
            .scrollbar(true)
            .child(
                button()
                    .w(200.)
                    .id("button")
                    .reactive_style(move || Styles::new().h(if read.get() { 400. } else { 100. }))
                    .on_click(move || {
                        click.set(click.get() + 1);
                    }),
            ),
    );
    ui.prepare_frame();
    let bar = bar(&ui);
    down(&mut ui, 50., 50.);
    up(&mut ui, 50., 50.);
    assert_eq!(calls.get(), 1);
    ui.input.focus(&ui.scene, Some(bar));
    tall.set(false);
    ui.prepare_frame();
    assert!(ui.semantics.borrow().get(bar).is_none());
    assert_ne!(ui.input.focused(), Some(bar));
    down(&mut ui, 196., 50.);
    up(&mut ui, 196., 50.);
    assert_eq!(calls.get(), 2);
    tall.set(true);
    ui.prepare_frame();
    assert_eq!(self::bar(&ui), bar);
    view.unmount();
    offset.set(1000.);
    assert_eq!(offset.get(), 1000.);
}
#[test]
fn virtual_scrollbar_tracks_extent_and_leaf_validation_cleans_mount() {
    let mut ui = Ui::new(500., 300.);
    let offset = ui.signal(0.);
    let count = ui.signal(100);
    let read = count.clone();
    let view = ui.mount(
        virtual_list(
            offset.clone(),
            20.,
            1,
            move || read.get(),
            |n| n,
            |_, n, _| text(n.to_string()),
        )
        .w(200.)
        .h(100.)
        .scrollbar(true),
    );
    ui.prepare_frame();
    let bar = bar(&ui);
    assert_eq!(ui.semantics.borrow().get(bar).unwrap().max, Some(1900.));
    ui.input
        .dispatch_to(&ui.scene, bar, InputEvent::SetValue("500".into()));
    assert_eq!(offset.get(), 500.);
    count.set(2);
    ui.prepare_frame();
    assert_eq!(offset.get(), 0.);
    assert!(ui.semantics.borrow().get(bar).is_none());
    let before = ui.scene.borrow().len();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(
            || ui.mount(column().child(text("bad").scrollbar(true)))
        ))
        .is_err()
    );
    assert_eq!(ui.scene.borrow().len(), before);
    assert!(view.is_mounted());
}

#[test]
fn thumb_drag_survives_unrelated_button_release() {
    let mut ui = Ui::new(500., 400.);
    let offset = ui.signal(0.);
    ui.mount(
        scroll(offset.clone())
            .size(200., 120.)
            .scrollbar(true)
            .child(div().h(600.)),
    );
    ui.prepare_frame();
    let track = bar(&ui);
    down(&mut ui, 196., 10.);
    assert_eq!(ui.input.captured(), Some(track));
    ui.dispatch(InputEvent::PointerDown {
        x: 196.,
        y: 10.,
        button: PointerButton::Secondary,
    });
    ui.dispatch(InputEvent::PointerUp {
        x: 196.,
        y: 10.,
        button: PointerButton::Secondary,
    });
    assert_eq!(ui.input.captured(), Some(track));
    ui.dispatch(InputEvent::PointerMove { x: 196., y: 70. });
    assert!(offset.get() > 0.);
    up(&mut ui, 196., 70.);
    assert_eq!(ui.input.captured(), None);
    let settled = offset.get();
    ui.dispatch(InputEvent::PointerMove { x: 196., y: 100. });
    assert_eq!(offset.get(), settled);
}
