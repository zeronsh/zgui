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

/// A name and value made from state are worked out only while assistive
/// technology listens: with none, the state changing calls neither; once
/// one listens, both are what the state is now, before the tree is read.
#[test]
fn names_from_state_wait_for_assistive_technology() {
    use std::{cell::Cell, rc::Rc};
    let mut ui = Ui::new(400., 200.);
    ui.set_assistive_technology(false);
    let count = ui.signal(1_u32);
    let calls = Rc::new(Cell::new(0));
    let view = ui.mount(
        column().child(
            div()
                .id("row")
                .size(100., 20.)
                .accessible_label_when({
                    let (count, calls) = (count.clone(), calls.clone());
                    move || {
                        calls.set(calls.get() + 1);
                        format!("{} unread", count.get())
                    }
                })
                .value_when({
                    let count = count.clone();
                    move || count.get().to_string()
                }),
        ),
    );
    ui.prepare_frame();
    let read = |ui: &Ui| {
        let node = view.find("row").unwrap();
        let semantics = ui.semantics.borrow();
        let node = semantics.get(node).unwrap();
        (node.label.clone(), node.value.clone())
    };
    for n in 2..10 {
        count.set(n);
        ui.prepare_frame();
    }
    assert_eq!(calls.get(), 0, "not worked out with no reader");
    // A reader asks for the tree: names and values are the state's now.
    ui.set_assistive_technology(true);
    assert_eq!(read(&ui), ("9 unread".into(), Some("9".into())));
    assert_eq!(calls.get(), 1);
    count.set(10);
    ui.prepare_frame();
    assert_eq!(
        read(&ui),
        ("10 unread".into(), Some("10".into())),
        "followed"
    );
    // It leaves: nothing more is worked out.
    ui.set_assistive_technology(false);
    let before = calls.get();
    count.set(11);
    ui.prepare_frame();
    assert_eq!(calls.get(), before);
}

/// Text a binding changes names its node and the button it is in only for
/// assistive technology: with none listening neither is written, and once
/// one listens both are the text as it is now, before the tree is read. A
/// node removed meanwhile is passed over.
#[test]
fn labels_from_changing_text_wait_for_assistive_technology() {
    let mut ui = Ui::new(400., 200.);
    ui.set_assistive_technology(false);
    let caption = ui.signal("Draft 1".to_string());
    let view = ui.mount(
        column()
            .child(button().id("save").child(text_signal({
                let caption = caption.clone();
                move || caption.get()
            })))
            .child(
                text_signal({
                    let caption = caption.clone();
                    move || format!("{} saved", caption.get())
                })
                .id("note"),
            ),
    );
    ui.prepare_frame();
    let label = |ui: &Ui| {
        let node = view.find("save").unwrap();
        ui.semantics.borrow().get(node).unwrap().label.clone()
    };
    let named = label(&ui);
    for n in 2..6 {
        caption.set(format!("Draft {n}"));
        ui.prepare_frame();
    }
    assert_eq!(label(&ui), named, "not written with no reader");
    // A pending node goes before anyone listens.
    let note = view.find("note").unwrap();
    ui.remove(note);
    ui.prepare_frame();
    ui.set_assistive_technology(true);
    assert_eq!(label(&ui), "Draft 5", "the text's now, at once");
    caption.set("Draft 6".into());
    ui.prepare_frame();
    assert_eq!(label(&ui), "Draft 6", "followed while listened to");
}

/// With no reader, text nodes removed while their labels wait are not kept
/// waiting: rows mounted and removed for a long session leave about as
/// many pending as live, and the ones still shown are right once one
/// listens.
#[test]
fn removed_text_leaves_no_label_waiting() {
    let mut ui = Ui::new(400., 200.);
    ui.set_assistive_technology(false);
    let round = ui.signal(0_u32);
    let caption = ui.signal(0_u32);
    let view = ui.mount(column().id("rows").child(switch(
        {
            let round = round.clone();
            move || round.get()
        },
        {
            let caption = caption.clone();
            move |_, _| {
                let mut rows = column();
                for i in 0..10 {
                    let caption = caption.clone();
                    rows = rows.child(
                        button()
                            .id(format!("row{i}"))
                            .child(text_signal(move || format!("row {i} at {}", caption.get()))),
                    );
                }
                rows
            }
        },
    )));
    for n in 1..300 {
        round.set(n);
        caption.set(n);
        ui.prepare_frame();
    }
    assert!(
        ui.pending_label_count() <= 2048,
        "{}",
        ui.pending_label_count()
    );
    ui.set_assistive_technology(true);
    let row = view.find("row3").unwrap();
    assert_eq!(
        ui.semantics.borrow().get(row).unwrap().label,
        "row 3 at 299"
    );
}
