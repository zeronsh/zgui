use std::{cell::Cell, rc::Rc};
use zgx::{
    compose::prelude::*,
    input::{InputEvent, Key, Modifiers, PointerButton},
    widgets::Ui,
};
fn escape(ui: &mut Ui) {
    ui.dispatch(InputEvent::KeyDown {
        key: Key::Escape,
        modifiers: Modifiers::default(),
        repeat: false,
    });
}
fn down(ui: &mut Ui, x: f32, y: f32) {
    ui.dispatch(InputEvent::PointerDown {
        x,
        y,
        button: PointerButton::Primary,
    });
}
#[test]
fn modal_portal_escapes_parent_clip_retains_children_and_recenters() {
    let mut ui = Ui::new(400., 300.);
    let open = ui.signal(false);
    let text_value = ui.signal("original".to_string());
    let view = ui.mount(
        column().w(40.).h(40.).overflow_hidden().child(
            modal("Settings", open.clone())
                .w(200.)
                .h(100.)
                .p(0.)
                .translate(3.5, 2.5)
                .id("panel")
                .child(
                    text_input("Name", text_value.clone())
                        .w(150.)
                        .h(30.)
                        .id("editor"),
                ),
        ),
    );
    ui.prepare_frame();
    let panel = view.find("panel").unwrap();
    let editor = view.find("editor").unwrap();
    assert_eq!(ui.scene.borrow().bounds(view.node()).height, 40.);
    assert!(ui.input.focus_scope().is_none());
    open.set(true);
    ui.prepare_frame();
    let bounds = ui.scene.borrow().bounds(panel);
    assert_eq!(
        (bounds.x, bounds.y, bounds.width, bounds.height),
        (103.5, 102.5, 200., 100.)
    );
    assert_eq!(ui.input.focused(), Some(editor));
    assert!(ui.semantics.borrow().get(panel).unwrap().modal);
    ui.scene.borrow_mut().resize(600., 400.);
    ui.prepare_frame();
    let bounds = ui.scene.borrow().bounds(panel);
    assert_eq!((bounds.x, bounds.y), (203.5, 152.5));
    open.set(false);
    text_value.set("closed update".into());
    open.set(true);
    ui.prepare_frame();
    assert_eq!(view.find("editor"), Some(editor));
    assert_eq!(
        ui.semantics.borrow().get(editor).unwrap().value.as_deref(),
        Some("closed update")
    );
}
#[test]
fn modal_focus_traps_dismisses_and_restores_without_click_through() {
    let mut ui = Ui::new(400., 300.);
    let open = ui.signal(false);
    let count = ui.signal(0);
    let calls = count.clone();
    let view = ui.mount(
        column()
            .child(button().w(50.).h(30.).id("trigger").on_click(move || {
                calls.set(calls.get() + 1);
            }))
            .child(
                modal("Confirm", open.clone())
                    .w(200.)
                    .h(100.)
                    .p(0.)
                    .child(button().w(60.).h(30.).id("first"))
                    .child(button().w(60.).h(30.).id("second")),
            ),
    );
    ui.prepare_frame();
    let trigger = view.find("trigger").unwrap();
    ui.input.focus(&ui.scene, Some(trigger));
    open.set(true);
    ui.prepare_frame();
    assert_eq!(ui.input.focused(), view.find("first"));
    assert!(!ui.input.focus(&ui.scene, Some(trigger)));
    down(&mut ui, 280., 180.);
    assert!(open.get()); // Blank panel area is not backdrop.
    escape(&mut ui);
    assert!(!open.get());
    assert_eq!(ui.input.focused(), Some(trigger));
    open.set(true);
    ui.prepare_frame();
    down(&mut ui, 20., 20.);
    ui.dispatch(InputEvent::PointerUp {
        x: 20.,
        y: 20.,
        button: PointerButton::Primary,
    });
    assert!(!open.get());
    assert_eq!(count.get(), 0);
    assert_eq!(ui.input.focused(), Some(trigger));
}
#[test]
fn escape_in_a_text_field_closes_its_dialog_unless_it_cancels_a_composition() {
    let mut ui = Ui::new(400., 300.);
    let open = ui.signal(false);
    let value = ui.signal(String::new());
    let view = ui.mount(
        column().child(button().w(50.).h(30.).id("trigger")).child(
            modal("New Message", open.clone())
                .w(300.)
                .h(120.)
                .child(text_input("To", value.clone()).id("to").w(200.)),
        ),
    );
    ui.prepare_frame();
    let trigger = view.find("trigger").unwrap();
    ui.input.focus(&ui.scene, Some(trigger));
    open.set(true);
    ui.prepare_frame();
    let field = view.find("to").unwrap();
    ui.input.focus(&ui.scene, Some(field));
    // Composing: Escape cancels the composition and the dialog stays.
    ui.dispatch(InputEvent::ImePreedit {
        text: "ㅎ".into(),
        cursor: Some((3, 3)),
    });
    escape(&mut ui);
    assert!(open.get(), "Escape cancelled the composition only");
    // Nothing composing: Escape closes the dialog, focus goes back.
    escape(&mut ui);
    assert!(!open.get());
    assert_eq!(ui.input.focused(), Some(trigger));
}

#[test]
fn tab_cycles_a_dialogs_controls_and_never_stops_on_the_dialog() {
    let mut ui = Ui::new(400., 300.);
    let open = ui.signal(false);
    let view = ui.mount(
        column().child(button().w(50.).h(30.).id("trigger")).child(
            modal("Confirm", open.clone())
                .w(200.)
                .h(100.)
                .child(button().w(60.).h(30.).id("first"))
                .child(button().w(60.).h(30.).id("second")),
        ),
    );
    ui.prepare_frame();
    open.set(true);
    ui.prepare_frame();
    let (first, second) = (view.find("first"), view.find("second"));
    let tab = |ui: &mut Ui, shift: bool| {
        ui.dispatch(InputEvent::KeyDown {
            key: Key::Tab,
            modifiers: Modifiers {
                shift,
                ..Default::default()
            },
            repeat: false,
        });
    };
    assert_eq!(ui.input.focused(), first);
    tab(&mut ui, false);
    assert_eq!(ui.input.focused(), second);
    tab(&mut ui, false);
    assert_eq!(
        ui.input.focused(),
        first,
        "past the last control, the first"
    );
    tab(&mut ui, true);
    assert_eq!(ui.input.focused(), second, "and back");
}

#[test]
fn nested_escape_closes_only_top_and_parent_close_unwinds_children() {
    let mut ui = Ui::new(400., 300.);
    let outer = ui.signal(false);
    let inner = ui.signal(false);
    let view = ui.mount(
        modal("Outer", outer.clone())
            .w(250.)
            .h(150.)
            .p(0.)
            .child(button().id("outer_button").child(text("Outer")))
            .child(
                modal("Inner", inner.clone())
                    .w(120.)
                    .h(80.)
                    .p(0.)
                    .child(button().id("inner_button").child(text("Inner"))),
            ),
    );
    outer.set(true);
    inner.set(true);
    ui.prepare_frame();
    assert_eq!(ui.input.focused(), view.find("inner_button"));
    escape(&mut ui);
    assert!(outer.get());
    assert!(!inner.get());
    assert_eq!(ui.input.focused(), view.find("outer_button"));
    inner.set(true);
    outer.set(false);
    assert!(!inner.get());
    assert!(ui.input.focus_scope().is_none());
}
#[test]
fn closing_or_removing_lower_sibling_keeps_top_and_repairs_focus_restore() {
    for remove in [false, true] {
        let mut ui = Ui::new(400., 300.);
        let trigger = ui.mount(button().child(text("Launch")));
        ui.prepare_frame();
        ui.input.focus(&ui.scene, Some(trigger.node()));
        let first_open = ui.signal(true);
        let first = ui.mount(modal("First", first_open.clone()).child(button().id("first")));
        let second_open = ui.signal(true);
        let second = ui.mount(modal("Second", second_open.clone()).child(button().id("second")));
        assert_eq!(ui.input.focused(), second.find("second"));
        let active = ui.input.focus_scope();
        if remove {
            first.unmount();
        } else {
            first_open.set(false);
        }
        assert_eq!(ui.input.focus_scope(), active);
        assert_eq!(ui.input.focused(), second.find("second"));
        second_open.set(false);
        assert!(ui.input.focus_scope().is_none());
        assert_eq!(ui.input.focused(), Some(trigger.node()));
    }
}
#[test]
fn render_replaces_open_portal_without_losing_new_scope() {
    let mut ui = Ui::new(400., 300.);
    let first = ui.render(modal("Old", ui.signal(true)).child(button().id("old")));
    let next_open = ui.signal(true);
    let next = ui.render(modal("New", next_open.clone()).child(button().id("new")));
    ui.prepare_frame();
    assert!(!first.is_mounted());
    assert!(next_open.get());
    assert_eq!(ui.input.focused(), next.find("new"));
    next.unmount();
    assert!(ui.input.focus_scope().is_none());
    assert_eq!(ui.scene.borrow().len(), 1);
}
struct CountDrop(Rc<Cell<usize>>);
impl Drop for CountDrop {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}
#[test]
fn portal_failure_cleans_nodes_scopes_and_owned_resources() {
    let mut ui = Ui::new(400., 300.);
    let prior = ui.mount(button().child(text("Prior")));
    ui.prepare_frame();
    ui.input.focus(&ui.scene, Some(prior.node()));
    let baseline = ui.scene.borrow().len();
    let drops = Rc::new(Cell::new(0));
    let owned = drops.clone();
    let open = ui.signal(true);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ui.mount(modal("Bad", open).child(component(move |cx| {
            cx.retain(CountDrop(owned));
            column()
                .child(text("Staged"))
                .child(component(|_| panic!("bad child")))
        })))
    }));
    assert!(result.is_err());
    assert_eq!(drops.get(), 1);
    assert_eq!(ui.scene.borrow().len(), baseline);
    assert_eq!(ui.input.focused(), Some(prior.node()));
    assert!(ui.input.focus_scope().is_none());
}

#[test]
fn nondismissing_backdrop_preserves_focus_and_nested_initial_open_is_ordered() {
    let mut ui = Ui::new(400., 300.);
    let outer = ui.signal(true);
    let inner = ui.signal(true);
    let view = ui.mount(
        modal("Outer", outer.clone())
            .child(button().id("outer"))
            .child(
                modal("Inner", inner.clone())
                    .dismiss_on_backdrop(false)
                    .child(button().id("inner")),
            ),
    );
    ui.prepare_frame();
    assert!(outer.get() && inner.get());
    assert_eq!(ui.input.focused(), view.find("inner"));
    down(&mut ui, 1., 1.);
    assert!(inner.get());
    assert_eq!(ui.input.focused(), view.find("inner"));
    escape(&mut ui);
    assert!(!inner.get());
    assert!(outer.get());
    assert_eq!(ui.input.focused(), view.find("outer"));
}
