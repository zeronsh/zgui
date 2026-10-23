//! Context menus: a secondary click opens one at the pointer; choosing an
//! item runs it and closes the menu; the innermost view's menu wins.
use std::{cell::RefCell, rc::Rc};
use zgui::{
    compose::prelude::*,
    input::{InputEvent, PointerButton},
    widgets::Ui,
};

fn click(ui: &Ui, x: f32, y: f32, button: PointerButton) {
    ui.input
        .dispatch(&ui.scene, InputEvent::PointerDown { x, y, button });
    ui.input
        .dispatch(&ui.scene, InputEvent::PointerUp { x, y, button });
    ui.prepare_frame();
}

#[test]
fn secondary_click_opens_items_at_the_pointer_and_runs_them() {
    let mut ui = Ui::new(400., 300.);
    let log = Rc::new(RefCell::new(Vec::<String>::new()));
    let (outer_log, inner_log) = (log.clone(), log.clone());
    let view = ui.mount(
        div()
            .id("outer")
            .size(400., 300.)
            .context_menu(move |_| {
                let log = outer_log.clone();
                vec![
                    menu_item("Outer")
                        .id("outer-item")
                        .on_click(move || log.borrow_mut().push("outer".into())),
                ]
            })
            .child(div().id("inner").size(100., 100.).context_menu(move |e| {
                let log = inner_log.clone();
                let at = format!("{},{}", e.local_x, e.local_y);
                vec![
                    menu_item("Inner")
                        .id("inner-item")
                        .on_click(move || log.borrow_mut().push(format!("inner {at}"))),
                ]
            })),
    );
    ui.prepare_frame();
    // On the inner view: its menu, at the pointer.
    click(&ui, 30., 40., PointerButton::Secondary);
    assert!(view.find("outer-item").is_none());
    let item = view.find("inner-item").expect("the inner menu shows");
    let b = ui.scene.borrow().bounds(item);
    assert!(
        b.x >= 30. && b.y >= 40. && b.x < 80. && b.y < 90.,
        "at the pointer: {b:?}"
    );
    click(&ui, b.x + 5., b.y + 5., PointerButton::Primary);
    assert_eq!(*log.borrow(), vec!["inner 30,40".to_string()]);
    // Elsewhere: the outer view's menu.
    click(&ui, 300., 200., PointerButton::Secondary);
    assert!(view.find("outer-item").is_some());
}

#[test]
fn a_menu_below_its_view_opens_under_it_wherever_the_click_was() {
    let mut ui = Ui::new(400., 300.);
    let view = ui.mount(
        div().size(400., 300.).child(
            div()
                .absolute()
                .left(50.)
                .top(20.)
                .size(120., 28.)
                .context_menu_below(true)
                .context_menu(|_| vec![menu_item("New Tab").id("item")]),
        ),
    );
    ui.prepare_frame();
    click(&ui, 140., 30., PointerButton::Secondary);
    let item = view.find("item").expect("the menu shows");
    let b = ui.scene.borrow().bounds(item);
    // Under the view's bottom edge (48), from its left edge, not the pointer.
    assert!(b.y >= 48. && b.y < 70., "under the view: {b:?}");
    assert!(b.x >= 50. && b.x < 70., "from its left edge: {b:?}");
}

#[test]
fn shift_f10_and_the_menu_key_open_it_under_what_has_focus() {
    use zgui::input::{Key, Modifiers};
    let mut ui = Ui::new(400., 300.);
    let view = ui.mount(
        div().p(40.).child(
            button()
                .id("file")
                .size(120., 30.)
                .child(text("Report.pdf"))
                .context_menu(|_| vec![menu_item("Rename").id("rename")]),
        ),
    );
    ui.prepare_frame();
    let file = view.find("file").unwrap();
    ui.input.focus(&ui.scene, Some(file));
    let press = |ui: &mut Ui, key: Key, shift: bool| {
        ui.dispatch(InputEvent::KeyDown {
            key,
            modifiers: Modifiers {
                shift,
                ..Default::default()
            },
            repeat: false,
        });
        ui.prepare_frame();
        ui.prepare_frame();
    };
    press(&mut ui, Key::Function(10), true);
    let item = view.find("rename").expect("open from Shift+F10");
    let (b, at) = {
        let scene = ui.scene.borrow();
        (scene.bounds(file), scene.bounds(item))
    };
    assert!(at.y >= b.y + b.height, "under the button: {at:?} {b:?}");
    press(&mut ui, Key::Escape, false);
    press(&mut ui, Key::ContextMenu, false);
    assert!(view.find("rename").is_some(), "and from the Menu key");
}
