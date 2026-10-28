//! An editor's input mode (HTML's `inputmode`): a hint of what kind of text
//! it takes, for the platform's input, never a check of what is typed.
use zgx::{compose::prelude::*, input::InputEvent, text_edit::InputMode, widgets::Ui};

#[test]
fn a_fields_input_mode_is_a_hint_not_a_check() {
    let mut ui = Ui::new(400., 200.);
    let value = ui.signal(String::new());
    let view = ui.mount(
        column()
            .child(
                text_input("Amount", value.clone())
                    .id("amount")
                    .input_mode(InputMode::Decimal),
            )
            .child(text_input("Name", value.clone()).id("name")),
    );
    ui.prepare_frame();
    let amount = ui.editor(view.find("amount").unwrap()).unwrap();
    assert_eq!(amount.input_mode(), InputMode::Decimal);
    assert_eq!(
        ui.editor(view.find("name").unwrap()).unwrap().input_mode(),
        InputMode::Text
    );
    // Letters still type: a hint, not a filter.
    ui.input.focus(&ui.scene, view.find("amount"));
    ui.dispatch(InputEvent::Text("abc".into()));
    assert_eq!(value.get(), "abc");
    amount.set_input_mode(InputMode::None);
    assert_eq!(amount.input_mode(), InputMode::None);
}

#[test]
#[should_panic(expected = "input_mode is only supported on editor views")]
fn only_editors_take_an_input_mode() {
    let mut ui = Ui::new(400., 200.);
    ui.mount(text("Total").input_mode(InputMode::Numeric));
}
