use zgx::{
    compose::prelude::*,
    input::{InputEvent, Key, Modifiers},
    widgets::Ui,
};
fn key(ui: &mut Ui, key: Key) {
    ui.dispatch(InputEvent::KeyDown {
        key,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    ui.prepare_frame();
}
#[test]
fn long_menu_reveals_keyboard_targets_and_scrolls_without_relayout() {
    let mut ui = Ui::new(300., 200.);
    let gap = ui.signal(3.);
    let read = gap.clone();
    let view = ui.mount(
        menu(
            "Long",
            ui.signal(true),
            button().size(60., 20.).child("Menu"),
        )
        .id("panel")
        .w(160.)
        .max_h(100.)
        .p(8.)
        .reactive_style(move || Styles::new().gap(read.get()))
        .children((0..20).map(|i| {
            menu_item(format!("Item {i}"))
                .id(format!("item{i}"))
                .h(20.)
                .p(0.)
        })),
    );
    ui.prepare_frame();
    let panel = view.find("panel").unwrap();
    let first = view.find("item0").unwrap();
    let second = view.find("item1").unwrap();
    let last = view.find("item19").unwrap();
    assert_eq!(ui.scene.borrow().bounds(panel).height, 100.);
    assert_eq!(
        ui.scene.borrow().bounds(second).y - ui.scene.borrow().bounds(first).y,
        23.
    );
    gap.set(7.);
    ui.prepare_frame();
    assert_eq!(
        ui.scene.borrow().bounds(second).y - ui.scene.borrow().bounds(first).y,
        27.
    );
    ui.scene.borrow_mut().flush();
    key(&mut ui, Key::End);
    assert_eq!(ui.input.focused(), Some(last));
    let bounds = ui.scene.borrow().bounds(panel);
    let item = ui.scene.borrow().bounds(last);
    assert!(
        item.y >= bounds.y + 8. && item.y + item.height <= bounds.y + bounds.height - 8. + 0.01
    );
    assert_eq!(ui.scene.borrow_mut().flush().layout_nodes, 0);
    ui.dispatch(InputEvent::Scroll {
        x: bounds.x + 20.,
        y: bounds.y + 20.,
        delta_x: 0.,
        delta_y: -20.,
    });
    ui.prepare_frame();
    assert_eq!(ui.scene.borrow().bounds(last).y, item.y + 20.);
    assert_eq!(ui.scene.borrow_mut().flush().layout_nodes, 0);
    key(&mut ui, Key::Home);
    assert_eq!(ui.input.focused(), Some(first));
    assert_eq!(ui.scene.borrow().bounds(first).y, bounds.y + 8.);
    view.unmount();
    assert_eq!(ui.scene.borrow().len(), 1);
    assert_eq!(ui.runtime.effect_count(), 0);
}
#[test]
fn dynamic_menu_shrinks_after_overflow_content_is_removed() {
    let mut ui = Ui::new(300., 200.);
    let rows = ui.signal((0..20).collect::<Vec<_>>());
    let read = rows.clone();
    let view = ui.mount(
        menu(
            "Dynamic",
            ui.signal(true),
            button().size(60., 20.).child("Menu"),
        )
        .id("panel")
        .w(160.)
        .max_h(100.)
        .p(8.)
        .child(keyed(
            move || read.get(),
            |i, _| {
                menu_item(format!("Item {i}"))
                    .id(format!("item{i}"))
                    .h(20.)
                    .p(0.)
            },
        )),
    );
    ui.prepare_frame();
    let first = view.find("item0").unwrap();
    let panel = view.find("panel").unwrap();
    assert_eq!(ui.scene.borrow().bounds(panel).height, 100.);
    key(&mut ui, Key::End);
    rows.set(vec![0]);
    ui.prepare_frame();
    assert_eq!(view.find("item0"), Some(first));
    assert_eq!(ui.scene.borrow().bounds(panel).height, 36.);
    assert_eq!(
        ui.scene.borrow().bounds(first).y,
        ui.scene.borrow().bounds(panel).y + 8.
    );
    view.unmount();
    assert_eq!(ui.runtime.effect_count(), 0);
}
#[test]
fn a_menu_the_platform_cut_to_its_room_is_laid_out_no_bigger_and_scrolls() {
    let mut ui = Ui::new(300., 200.);
    // As on Wayland: screens unbounded, the compositor the judge of the room.
    ui.set_popup_screens(vec![zgx::scene::Rect::new(-1e5, -1e5, 2e5, 2e5)]);
    let open = ui.signal(false);
    let view = ui.mount(
        menu("Long", open.clone(), button().size(60., 20.).child("Menu"))
            .id("panel")
            .w(160.)
            .p(8.)
            .children((0..60).map(|i| {
                menu_item(format!("Item {i}"))
                    .id(format!("item{i}"))
                    .h(20.)
                    .p(0.)
            })),
    );
    ui.prepare_frame();
    open.set(true);
    ui.prepare_frame();
    let root = *ui.scene.borrow().popups().first().expect("a popup root");
    let panel = view.find("panel").unwrap();
    let full = ui.scene.borrow().bounds(panel);
    assert_eq!(full.height, 60. * 20. + 16.);
    // The compositor gave it 300 points: laid out no taller, it scrolls.
    ui.set_popup_limit(root, Some((full.width, 300.)));
    ui.prepare_frame();
    let cut = ui.scene.borrow().bounds(panel);
    assert_eq!((cut.width, cut.height), (160., 300.));
    assert_eq!(ui.scene.borrow().popup_bounds(root).unwrap().height, 300.);
    key(&mut ui, Key::End);
    let last = view.find("item59").unwrap();
    assert_eq!(ui.input.focused(), Some(last));
    let item = ui.scene.borrow().bounds(last);
    assert!(item.y >= cut.y + 8. && item.y + item.height <= cut.y + cut.height - 8. + 0.01);
    // Room to spare: its own height again.
    ui.set_popup_limit(root, Some((full.width, 2000.)));
    ui.prepare_frame();
    assert_eq!(ui.scene.borrow().bounds(panel).height, full.height);
    // Closed, it is no longer cut.
    ui.set_popup_limit(root, None);
    open.set(false);
    ui.prepare_frame();
    open.set(true);
    ui.prepare_frame();
    assert_eq!(ui.scene.borrow().bounds(panel).height, full.height);
    view.unmount();
    assert_eq!(ui.runtime.effect_count(), 0);
}
