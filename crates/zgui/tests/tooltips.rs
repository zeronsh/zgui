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
