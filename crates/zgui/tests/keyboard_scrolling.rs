//! The keyboard scrolls the scroll view in use, as a page does, when
//! nothing else takes the key.
use std::{cell::Cell, rc::Rc};
use zgui::{
    compose::prelude::*,
    input::{InputEvent, Key, Modifiers, PointerButton},
    widgets::Ui,
};

fn key(ui: &mut Ui, key: Key, shift: bool) -> bool {
    ui.dispatch(InputEvent::KeyDown {
        key,
        modifiers: Modifiers {
            shift,
            ..Modifiers::default()
        },
        repeat: false,
    })
    .default_prevented
}

#[test]
fn page_space_line_and_end_keys_scroll_the_view_in_use() {
    let mut ui = Ui::new(400., 300.);
    let offset = ui.signal(0.);
    let pressed = Rc::new(Cell::new(0));
    let count = pressed.clone();
    let value = ui.signal(String::new());
    ui.mount(
        column()
            .child(
                scroll(offset.clone())
                    .w(300.)
                    .h(200.)
                    .child(column().children((0..100).map(|i| text(format!("Line {i}")).h(20.)))),
            )
            .child(
                button()
                    .id("button")
                    .child(text("Press"))
                    .on_click(move || count.set(count.get() + 1)),
            )
            .child(text_input("Field", value).w(200.)),
    );
    ui.prepare_frame();
    // Nothing pressed or hovered yet: no scroll view in use.
    assert!(!key(&mut ui, Key::PageDown, false));
    ui.dispatch(InputEvent::PointerDown {
        x: 10.,
        y: 10.,
        button: PointerButton::Primary,
    });
    ui.dispatch(InputEvent::PointerUp {
        x: 10.,
        y: 10.,
        button: PointerButton::Primary,
    });
    let at = |ui: &mut Ui| {
        ui.prepare_frame();
        offset.get()
    };
    assert!(key(&mut ui, Key::PageDown, false));
    assert_eq!(at(&mut ui), 175., "a page: 87.5% of the viewport");
    key(&mut ui, Key::Space, false);
    assert_eq!(at(&mut ui), 350.);
    key(&mut ui, Key::Space, true);
    assert_eq!(at(&mut ui), 175.);
    key(&mut ui, Key::ArrowDown, false);
    assert_eq!(at(&mut ui), 215., "a line: 40 points");
    key(&mut ui, Key::End, false);
    assert_eq!(at(&mut ui), 1800.);
    key(&mut ui, Key::Home, false);
    assert_eq!(at(&mut ui), 0.);
    // A focused control keeps its keys: a button its Space, an editor
    // its End.
    let view_button = {
        let scene = ui.scene.borrow();
        let column = scene.children(scene.root())[0];
        scene.children(column)[1]
    };
    ui.input.focus(&ui.scene, Some(view_button));
    key(&mut ui, Key::Space, false);
    ui.dispatch(InputEvent::KeyUp {
        key: Key::Space,
        modifiers: Modifiers::default(),
    });
    assert_eq!(pressed.get(), 1);
    assert_eq!(at(&mut ui), 0.);
    let field = {
        let scene = ui.scene.borrow();
        let column = scene.children(scene.root())[0];
        scene.children(column)[2]
    };
    ui.input.focus(&ui.scene, Some(field));
    key(&mut ui, Key::End, false);
    assert_eq!(at(&mut ui), 0.);
}
