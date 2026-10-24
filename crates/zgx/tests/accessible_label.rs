//! `View::accessible_label`: a name given for assistive technology stays the
//! node's name whatever text it shows.
use zgx::{compose::prelude::*, semantics::Role, widgets::Ui};

#[test]
fn a_given_name_replaces_the_one_made_from_text() {
    let mut ui = Ui::new(400., 200.);
    let caption = ui.signal("on".to_string());
    let view = ui.mount(
        column()
            .child(
                button()
                    .id("switch")
                    .accessible_label("Turn off")
                    .child(text_signal({
                        let caption = caption.clone();
                        move || caption.get()
                    })),
            )
            .child(button().id("save").child(text("Save")))
            .child(
                div()
                    .id("chart")
                    .accessible_label("CPU history")
                    .size(40., 20.),
            ),
    );
    ui.prepare_frame();
    let label = |ui: &Ui, id: &str| {
        let node = view.find(id).unwrap();
        let semantics = ui.semantics.borrow();
        let node = semantics.get(node).unwrap();
        (node.role, node.label.clone())
    };
    assert_eq!(label(&ui, "switch"), (Role::Button, "Turn off".into()));
    assert_eq!(
        label(&ui, "save"),
        (Role::Button, "Save".into()),
        "text still names a button"
    );
    assert_eq!(label(&ui, "chart"), (Role::Group, "CPU history".into()));
    caption.set("off".into());
    ui.prepare_frame();
    assert_eq!(
        label(&ui, "switch").1,
        "Turn off",
        "the text changing keeps the given name"
    );
}
