use std::{cell::RefCell, rc::Rc, time::Duration};
use zgui::{
    compose::{TaskRunner, Tooltip, prelude::*},
    input::{InputEvent, Key, Modifiers},
    scene::NodeKind,
    task::LocalExecutor,
    widgets::Ui,
};

/// Whether text `label` shows (laid out, on screen, not faded out).
fn showing(ui: &Ui, label: &str) -> bool {
    let scene = ui.scene.borrow();
    scene.paint_items().any(|item| {
        matches!(scene.kind(item.id), NodeKind::Text { text, .. } if &**text == label)
            && item.effects.opacity > 0.
            && scene.visible_bounds(item.id).is_some_and(|b| b.width > 0.)
    })
}

fn hover(ui: &mut Ui, x: f32, y: f32) {
    ui.dispatch(InputEvent::PointerMove { x, y });
    ui.prepare_frame();
}

fn key(ui: &mut Ui, key: Key) {
    ui.dispatch(InputEvent::KeyDown {
        key,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    ui.prepare_frame();
}

#[test]
fn a_tooltip_shows_on_hover_and_goes_as_the_pointer_leaves_or_a_key_is_pressed() {
    // No task runner: no delays or fades, it shows at once.
    let mut ui = Ui::new(300., 200.);
    ui.mount(
        column()
            .p(60.)
            .gap(20.)
            .child(
                button()
                    .size(60., 24.)
                    .child(text("Copy"))
                    .tooltip("Copy link"),
            )
            .child(div().size(60., 24.)),
    );
    ui.prepare_frame();
    assert!(!showing(&ui, "Copy link"));
    hover(&mut ui, 70., 70.);
    assert!(showing(&ui, "Copy link"), "over it");
    hover(&mut ui, 70., 110.);
    assert!(!showing(&ui, "Copy link"), "gone as the pointer leaves");
    hover(&mut ui, 70., 70.);
    assert!(showing(&ui, "Copy link"));
    key(&mut ui, Key::Character("x".into()));
    assert!(!showing(&ui, "Copy link"), "a key closes it");
}

#[test]
fn a_tooltip_waits_for_the_pointer_to_rest_and_an_interactive_card_takes_typing() {
    let mut ui = Ui::new(300., 200.);
    let executor = Rc::new(RefCell::new(LocalExecutor::new()));
    ui.mount(provide(
        TaskRunner::from_executor(executor.clone()),
        column()
            .p(60.)
            .gap(20.)
            .child(div().size(60., 24.).tooltip("Plain"))
            .child(div().size(60., 24.).tooltip(
                Tooltip::from(column().size(80., 40.).child(text("Card"))).interactive(true),
            )),
    ));
    ui.prepare_frame();
    // Until `done`, or `ms` have passed: whether it happened. Timers run on
    // a thread of their own, which a loaded machine runs late.
    let until = |ui: &mut Ui, ms: u64, done: &dyn Fn(&Ui) -> bool| {
        let end = std::time::Instant::now() + Duration::from_millis(ms);
        loop {
            while executor.borrow_mut().tick() > 0 {}
            ui.prepare_frame();
            if done(ui) {
                return true;
            }
            if std::time::Instant::now() >= end {
                return false;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    hover(&mut ui, 70., 70.);
    assert!(!showing(&ui, "Plain"), "not at once");
    assert!(
        until(&mut ui, 2000, &|ui| showing(ui, "Plain")),
        "after a rest"
    );
    // Within a second of it, the next opens at once: before a rest (300 ms)
    // would have shown it.
    hover(&mut ui, 70., 110.);
    assert!(until(&mut ui, 250, &|ui| showing(ui, "Card")), "at once");
    assert!(
        until(&mut ui, 2000, &|ui| !showing(ui, "Plain")),
        "the last faded out"
    );
    key(&mut ui, Key::Character("x".into()));
    assert!(showing(&ui, "Card"), "a card takes typing");
    key(&mut ui, Key::Escape);
    assert!(
        until(&mut ui, 2000, &|ui| !showing(ui, "Card")),
        "and closes on Escape"
    );
}

#[test]
fn a_tooltip_describes_its_view_and_shows_on_keyboard_focus() {
    use zgui::{input::PointerButton, semantics::Role};
    let mut ui = Ui::new(300., 200.);
    let view = ui.mount(
        column()
            .p(60.)
            .gap(20.)
            .child(
                button()
                    .id("copy")
                    .size(60., 24.)
                    .child(text("Copy"))
                    .tooltip("Copy link"),
            )
            .child(button().id("other").size(60., 24.).child(text("Other"))),
    );
    ui.prepare_frame();
    let copy = view.find("copy").unwrap();
    {
        let semantics = ui.semantics.borrow();
        let node = semantics.get(copy).unwrap();
        assert_eq!(node.role, Role::Button);
        assert!(!node.hidden, "the view itself stays");
        assert_eq!(node.description.as_deref(), Some("Copy link"));
    }
    // Tabbed to, it shows; tabbed past, it goes.
    key(&mut ui, Key::Tab);
    assert_eq!(ui.input.focused(), Some(copy));
    assert!(showing(&ui, "Copy link"), "shown on keyboard focus");
    key(&mut ui, Key::Tab);
    assert!(!showing(&ui, "Copy link"), "gone as focus leaves");
    // Its tip is not in the accessibility tree: no dialog of its own.
    key(&mut ui, Key::Tab);
    key(&mut ui, Key::Tab);
    let dialogs = ui
        .semantics
        .borrow()
        .iter()
        .filter(|(_, n)| n.role == Role::Dialog && !n.hidden)
        .count();
    assert_eq!(dialogs, 0);
    // A click focuses it without showing it (the press hides it anyway).
    let b = ui.scene.borrow().bounds(view.find("other").unwrap());
    let (x, y) = (b.x + 5., b.y + 5.);
    ui.dispatch(InputEvent::PointerMove { x, y });
    let button = PointerButton::Primary;
    ui.dispatch(InputEvent::PointerDown { x, y, button });
    ui.dispatch(InputEvent::PointerUp { x, y, button });
    ui.prepare_frame();
    assert!(!showing(&ui, "Copy link"));
}

#[test]
fn a_popovers_trigger_says_it_opens_a_dialog_and_whether_it_shows() {
    use zgui::semantics::{PopupKind, Role};
    let mut ui = Ui::new(400., 300.);
    let open = ui.signal(false);
    let view = ui.mount(
        column().p(20.).child(
            popover(
                "Filters",
                open.clone(),
                button().id("trigger").size(80., 28.),
            )
            .w(200.)
            .child(text("Panel")),
        ),
    );
    ui.prepare_frame();
    let trigger = view.find("trigger").unwrap();
    let state = |ui: &Ui| {
        let semantics = ui.semantics.borrow();
        let node = semantics.get(trigger).unwrap();
        (node.has_popup, node.expanded)
    };
    open.set(true);
    ui.prepare_frame();
    assert_eq!(state(&ui), (Some(PopupKind::Dialog), Some(true)));
    let modal = ui
        .semantics
        .borrow()
        .iter()
        .any(|(_, n)| n.role == Role::Dialog && n.modal);
    assert!(modal, "it traps focus, so it is modal to assistive technology");
    open.set(false);
    ui.prepare_frame();
    assert_eq!(state(&ui), (Some(PopupKind::Dialog), Some(false)));
}

#[test]
fn a_menu_matching_its_trigger_is_as_wide_as_it() {
    let mut ui = Ui::new(500., 300.);
    let open = ui.signal(true);
    let view = ui.mount(
        column().p(20.).child(
            menu("Size", open, button().id("trigger").size(220., 28.))
                .id("menu")
                .match_anchor_width()
                .child(menu_item("S")),
        ),
    );
    ui.prepare_frame();
    ui.prepare_frame();
    let scene = ui.scene.borrow();
    let (trigger, panel) = (
        scene.bounds(view.find("trigger").unwrap()),
        scene.bounds(view.find("menu").unwrap()),
    );
    assert!(panel.width >= trigger.width, "{panel:?} {trigger:?}");
}
