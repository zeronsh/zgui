//! A dialog from the keyboard, as a web `<dialog>`: Tab goes round what it
//! holds, and Escape closes it from anywhere in it, a text field included,
//! giving focus back.
use zgui::{
    compose::prelude::*,
    input::{InputEvent, Key, Modifiers},
    widgets::Ui,
};

fn press(ui: &mut Ui, key: Key) {
    ui.dispatch(InputEvent::KeyDown {
        key,
        modifiers: Modifiers::default(),
        repeat: false,
    });
}

#[test]
fn tab_stays_in_the_dialog_and_escape_in_a_field_closes_it() {
    let mut ui = Ui::new(600., 400.);
    let open = ui.signal(false);
    let value = ui.signal(String::new());
    let opener = open.clone();
    let view = ui.mount(
        column()
            .child(button().id("open").child(text("Open")).on_click(move || {
                opener.set(true);
            }))
            .child(
                modal("Settings", open.clone())
                    .size(300., 200.)
                    .child(text_input("Name", value).id("name").w(200.))
                    .child(button().id("ok").child(text("OK"))),
            ),
    );
    ui.prepare_frame();
    let opener = view.find("open").unwrap();
    ui.input.focus(&ui.scene, Some(opener));
    open.set(true);
    ui.prepare_frame();
    let (name, ok) = (view.find("name").unwrap(), view.find("ok").unwrap());
    // Round the field, the button and the dialog itself; never the opener.
    let mut seen = Vec::new();
    for _ in 0..6 {
        press(&mut ui, Key::Tab);
        seen.push(ui.input.focused());
    }
    assert!(!seen.contains(&Some(opener)), "{seen:?}");
    assert!(
        seen.contains(&Some(name)) && seen.contains(&Some(ok)),
        "{seen:?}"
    );
    ui.input.focus(&ui.scene, Some(name));
    press(&mut ui, Key::Escape);
    ui.prepare_frame();
    assert!(!open.get(), "Escape in the field closes the dialog");
    assert_eq!(ui.input.focused(), Some(opener), "focus goes back");
}
