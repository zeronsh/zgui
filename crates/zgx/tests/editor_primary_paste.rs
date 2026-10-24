//! Middle-click paste: a middle press puts an editor's caret under the
//! pointer and asks the host to paste the primary selection there.
#![cfg(all(unix, not(target_os = "macos")))]
use zgx::{
    compose::prelude::*,
    input::{InputEvent, PointerButton},
    widgets::Ui,
};

fn middle(ui: &mut Ui, x: f32, y: f32) {
    ui.dispatch(InputEvent::PointerDown {
        x,
        y,
        button: PointerButton::Middle,
    });
    ui.dispatch(InputEvent::PointerUp {
        x,
        y,
        button: PointerButton::Middle,
    });
}

#[test]
fn a_middle_press_places_the_caret_and_asks_for_the_primary_selection() {
    let mut ui = Ui::new(400., 200.);
    let value = ui.signal("abcdefghij".to_string());
    let locked = ui.signal("locked".to_string());
    let view = ui.mount(
        column()
            .child(text_input("Field", value).id("field").w(300.).h(30.))
            .child(
                text_input("Locked", locked)
                    .read_only(true)
                    .id("locked")
                    .w(300.)
                    .h(30.),
            ),
    );
    ui.prepare_frame();
    let field = view.find("field").unwrap();
    let editor = ui.editor(field).unwrap();
    editor.set_selection(0, 10);
    let bounds = ui.scene.borrow().bounds(field);
    middle(
        &mut ui,
        bounds.x + bounds.width - 2.,
        bounds.y + bounds.height / 2.,
    );
    assert_eq!(
        ui.take_primary_paste(),
        Some(field),
        "the host is asked once"
    );
    assert_eq!(ui.take_primary_paste(), None);
    assert_eq!(
        editor.selection(),
        (10, 10),
        "the caret went under the pointer, at the end"
    );
    middle(&mut ui, bounds.x + 2., bounds.y + bounds.height / 2.);
    assert_eq!(ui.take_primary_paste(), Some(field));
    assert_eq!(editor.selection(), (0, 0));

    let locked = view.find("locked").unwrap();
    let bounds = ui.scene.borrow().bounds(locked);
    middle(&mut ui, bounds.x + 4., bounds.y + bounds.height / 2.);
    assert_eq!(
        ui.take_primary_paste(),
        None,
        "a read-only field takes no paste"
    );
}
