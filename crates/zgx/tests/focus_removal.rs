//! When what has focus leaves the interface, focus moves on, as a native
//! toolkit's does: to its nearest focusable ancestor that stays, else to the
//! next Tab stop (a focus group's own), else the one before; never to the
//! window, which a screen reader would announce.
use zgx::{
    compose::{ViewHandle, prelude::*},
    semantics::Role,
    widgets::Ui,
};

fn named(id: &str) -> View {
    button().id(id).child(text(id))
}
/// `id`'s button while `shown` says so.
fn shown_named(shown: zgx::reactive::Signal<bool>, id: &'static str) -> View {
    switch(
        move || shown.get(),
        move |on, _| if on { named(id) } else { div() },
    )
}
fn focused(ui: &Ui, view: &ViewHandle, names: &[&str]) -> Option<String> {
    let id = ui.input.focused()?;
    names
        .iter()
        .find(|name| view.find(name) == Some(id))
        .map(|name| name.to_string())
}

#[test]
fn focus_moves_on_to_the_next_stop_or_back_to_the_one_before() {
    let mut ui = Ui::new(400., 300.);
    let shown = ui.signal(true);
    let after = ui.signal(true);
    let view = ui.mount(
        column()
            .child(named("before"))
            .child(shown_named(shown.clone(), "gone"))
            .child(shown_named(after.clone(), "after")),
    );
    ui.prepare_frame();
    let names = ["before", "gone", "after"];
    ui.input.focus(&ui.scene, view.find("gone"));
    shown.set(false);
    ui.prepare_frame();
    assert_eq!(focused(&ui, &view, &names).as_deref(), Some("after"));
    // Nothing after it: back to the one before.
    shown.set(true);
    ui.prepare_frame();
    ui.input.focus(&ui.scene, view.find("gone"));
    after.set(false);
    shown.set(false);
    ui.prepare_frame();
    assert_eq!(focused(&ui, &view, &names).as_deref(), Some("before"));
}

#[test]
fn a_focus_group_next_is_entered_at_its_own_stop() {
    let mut ui = Ui::new(400., 300.);
    let shown = ui.signal(true);
    let view = ui.mount(
        column().child(shown_named(shown.clone(), "gone")).child(
            row()
                .role(Role::Toolbar)
                .child(named("a"))
                .child(named("b")),
        ),
    );
    ui.prepare_frame();
    let names = ["gone", "a", "b"];
    // "b" was where focus last was in the toolbar.
    ui.input.focus(&ui.scene, view.find("b"));
    ui.input.focus(&ui.scene, view.find("gone"));
    shown.set(false);
    ui.prepare_frame();
    assert_eq!(focused(&ui, &view, &names).as_deref(), Some("b"));
}
