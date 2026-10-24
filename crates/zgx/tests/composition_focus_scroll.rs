use zgx::{
    compose::prelude::*,
    input::{InputEvent, Key, Modifiers},
    widgets::Ui,
};
fn tab(ui: &mut Ui, shift: bool) {
    ui.dispatch(InputEvent::KeyDown {
        key: Key::Tab,
        modifiers: Modifiers {
            shift,
            ..Default::default()
        },
        repeat: false,
    });
}
#[test]
fn tab_reveals_padded_viewport_and_manual_scroll_stays_put() {
    let mut ui = Ui::new(500., 300.);
    let offset = ui.signal(0.);
    let view =
        ui.mount(scroll(offset.clone()).w(200.).h(120.).p(10.).children(
            (0..5).map(|n| button().h(40.).id(n.to_string()).child(text(n.to_string()))),
        ));
    for _ in 0..5 {
        tab(&mut ui, false);
    }
    assert_eq!(ui.input.focused(), view.find("4"));
    assert_eq!(offset.get(), 100.);
    for _ in 0..4 {
        tab(&mut ui, true);
    }
    assert_eq!(offset.get(), 0.);
    ui.dispatch(InputEvent::Scroll {
        x: 20.,
        y: 20.,
        delta_x: 0.,
        delta_y: 70.,
    });
    assert_eq!(offset.get(), 70.);
    ui.prepare_frame();
    assert_eq!(offset.get(), 70.);
    assert_eq!(ui.input.focused(), view.find("0"));
}
#[test]
fn nested_focus_reveal_uses_immediate_inner_translation() {
    let mut ui = Ui::new(500., 400.);
    let outer = ui.signal(0.);
    let inner = ui.signal(0.);
    let view = ui.mount(
        scroll(outer.clone())
            .w(250.)
            .h(120.)
            .p(10.)
            .child(div().h(150.))
            .child(
                scroll(inner.clone())
                    .w(200.)
                    .h(100.)
                    .p(10.)
                    .child(div().h(200.))
                    .child(button().h(40.).id("target").child(text("Target"))),
            ),
    );
    let target = view.find("target").unwrap();
    // No explicit prepare_frame: direct focus must settle allocations itself.
    assert!(ui.input.focus(&ui.scene, Some(target)));
    assert_eq!(inner.get(), 160.);
    assert_eq!(outer.get(), 140.);
    let bounds = ui.scene.borrow().bounds(target);
    assert_eq!((bounds.y, bounds.height), (70., 40.));
    ui.input.focus(&ui.scene, None);
    inner.set(0.);
    outer.set(0.);
    ui.runtime.batch(|| {
        ui.input.focus(&ui.scene, Some(target));
    });
    assert_eq!(inner.get(), 160.);
    assert_eq!(outer.get(), 140.);
}
#[test]
fn oversized_focus_target_uses_nearest_edge_without_alternating() {
    let mut ui = Ui::new(400., 300.);
    let offset = ui.signal(0.);
    let view = ui.mount(
        scroll(offset.clone())
            .w(200.)
            .h(100.)
            .child(button().h(30.).id("first"))
            .child(div().h(120.))
            .child(button().h(200.).id("large"))
            .child(div().h(200.)),
    );
    let large = view.find("large").unwrap();
    assert!(ui.input.focus(&ui.scene, Some(large)));
    assert_eq!(offset.get(), 150.);
    ui.input.focus(&ui.scene, None);
    assert!(ui.input.focus(&ui.scene, Some(large)));
    assert_eq!(offset.get(), 150.);
    offset.set(200.);
    ui.input.focus(&ui.scene, None);
    ui.input.focus(&ui.scene, Some(large));
    assert_eq!(offset.get(), 200.);
    offset.set(450.);
    ui.input.focus(&ui.scene, None);
    ui.input.focus(&ui.scene, Some(large));
    assert_eq!(offset.get(), 250.);
}
#[test]
fn disabled_focus_rejected_and_unmounted_reveal_handler_released() {
    let mut ui = Ui::new(400., 300.);
    let offset = ui.signal(0.);
    let disabled = ui.signal(true);
    let read = disabled.clone();
    let view = ui.mount(
        scroll(offset.clone())
            .w(200.)
            .h(100.)
            .disabled_when(move || read.get())
            .child(div().h(200.))
            .child(button().h(40.).id("target")),
    );
    let target = view.find("target").unwrap();
    assert!(!ui.input.focus(&ui.scene, Some(target)));
    assert_eq!(offset.get(), 0.);
    disabled.set(false);
    assert!(ui.input.focus(&ui.scene, Some(target)));
    assert_eq!(offset.get(), 140.);
    view.unmount();
    assert!(!ui.input.has_listeners(view.node()));
    offset.set(25.);
    ui.prepare_frame();
    assert_eq!(offset.get(), 25.);
}

#[test]
fn redirected_focus_does_not_reveal_stale_event_target() {
    let mut ui = Ui::new(400., 300.);
    let offset = ui.signal(0.);
    let view = ui.mount(
        scroll(offset.clone())
            .w(200.)
            .h(100.)
            .child(button().h(40.).id("first"))
            .child(div().h(200.))
            .child(button().h(40.).id("redirect")),
    );
    let first = view.find("first").unwrap();
    let redirect = view.find("redirect").unwrap();
    let input = ui.input.clone();
    let scene = ui.scene.clone();
    ui.on_event(redirect, true, move |cx| {
        if matches!(cx.event, InputEvent::Focus) {
            input.focus(&scene, Some(first));
        }
    });
    assert!(ui.input.focus(&ui.scene, Some(redirect)));
    assert_eq!(ui.input.focused(), Some(first));
    assert_eq!(offset.get(), 0.);
}
