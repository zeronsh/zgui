//! Toasts: shown from anywhere, stacked, dismissed by their × or their
//! time (paused under the pointer), announced, and reached with F8.
use std::{cell::RefCell, rc::Rc, time::Duration};
use zgui::{
    compose::{TaskRunner, ViewHandle, prelude::*},
    input::{InputEvent, Key, Modifiers, PointerButton},
    scene::{NodeId, NodeKind},
    semantics::Role,
    task::LocalExecutor,
    widgets::Ui,
};
use zgui_components::*;

fn host(ui: &mut Ui, executor: &Rc<RefCell<LocalExecutor>>) -> ViewHandle {
    let view = ui.mount(provide(
        TaskRunner::from_executor(executor.clone()),
        column()
            .w_full()
            .h_full()
            .child(text("App"))
            .child(toast_host()),
    ));
    ui.prepare_frame();
    view
}
fn texts(ui: &Ui) -> Vec<String> {
    let scene = ui.scene.borrow();
    let mut out = Vec::new();
    let mut pending = vec![scene.root()];
    while let Some(n) = pending.pop() {
        if let NodeKind::Text { text, .. } = scene.kind(n) {
            out.push(text.to_string());
        }
        pending.extend(scene.children(n).iter().copied());
    }
    out
}
fn with_role(ui: &Ui, role: Role) -> Vec<NodeId> {
    let scene = ui.scene.borrow();
    let semantics = ui.semantics.borrow();
    let mut out = Vec::new();
    let mut pending = vec![scene.root()];
    while let Some(n) = pending.pop() {
        if semantics.get(n).is_some_and(|s| s.role == role) {
            out.push(n);
        }
        pending.extend(scene.children(n).iter().rev().copied());
    }
    out
}

#[test]
fn toasts_show_stack_and_go_by_their_x() {
    let mut ui = Ui::new(800., 600.);
    let executor = Rc::new(RefCell::new(LocalExecutor::new()));
    host(&mut ui, &executor);
    let toaster = Toaster::of(&ui.runtime);
    let undone = Rc::new(RefCell::new(false));
    let flag = undone.clone();
    toaster.show(Toast::new("Saved").body("Report.pdf"));
    let failed = toaster.show(Toast::new("Upload failed").kind(ToastKind::Error));
    toaster.show(Toast::new("Moved to Trash").action("Undo", move || *flag.borrow_mut() = true));
    ui.prepare_frame();
    ui.prepare_frame();
    let shown = texts(&ui);
    for t in [
        "Saved",
        "Report.pdf",
        "Upload failed",
        "Moved to Trash",
        "Undo",
    ] {
        assert!(shown.contains(&t.to_owned()), "{t} in {shown:?}");
    }
    // Announced: the error at once, the others politely.
    assert_eq!(with_role(&ui, Role::Alert).len(), 1);
    assert_eq!(with_role(&ui, Role::Status).len(), 2);
    // In the window's bottom right corner.
    let card = with_role(&ui, Role::Alert)[0];
    let b = ui.scene.borrow().bounds(card);
    assert!(b.x > 400. && b.y > 300., "{b:?}");
    // Its action runs, and it goes.
    let undo = with_role(&ui, Role::Button)
        .into_iter()
        .find(|n| ui.semantics.borrow().get(*n).unwrap().label == "Undo")
        .unwrap();
    let ub = ui.scene.borrow().bounds(undo);
    let (x, y) = (ub.x + 4., ub.y + 4.);
    let button = PointerButton::Primary;
    ui.dispatch(InputEvent::PointerDown { x, y, button });
    ui.dispatch(InputEvent::PointerUp { x, y, button });
    ui.prepare_frame();
    assert!(*undone.borrow());
    assert_eq!(toaster.showing().len(), 2);
    toaster.dismiss(failed);
    ui.prepare_frame();
    assert_eq!(
        toaster
            .showing()
            .iter()
            .map(|t| t.1.clone())
            .collect::<Vec<_>>(),
        ["Saved"]
    );
}

#[test]
fn a_toast_goes_after_its_time_but_not_under_the_pointer() {
    let mut ui = Ui::new(800., 600.);
    let executor = Rc::new(RefCell::new(LocalExecutor::new()));
    host(&mut ui, &executor);
    let toaster = Toaster::of(&ui.runtime);
    let wait = |ui: &mut Ui, ms: u64| {
        let until = std::time::Instant::now() + Duration::from_millis(ms);
        while std::time::Instant::now() < until {
            while executor.borrow_mut().tick() > 0 {}
            ui.prepare_frame();
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    toaster.show(Toast::new("Brief").duration(Duration::from_millis(80)));
    ui.prepare_frame();
    ui.prepare_frame();
    let card = with_role(&ui, Role::Status)[0];
    let b = ui.scene.borrow().bounds(card);
    ui.dispatch(InputEvent::PointerMove {
        x: b.x + 20.,
        y: b.y + 10.,
    });
    wait(&mut ui, 250);
    assert_eq!(
        toaster.showing().len(),
        1,
        "held while the pointer is over it"
    );
    ui.dispatch(InputEvent::PointerMove { x: 10., y: 10. });
    wait(&mut ui, 800);
    assert!(toaster.showing().is_empty(), "gone once the pointer left");
}

#[test]
fn f8_reaches_the_stack_and_escape_dismisses() {
    let mut ui = Ui::new(800., 600.);
    let executor = Rc::new(RefCell::new(LocalExecutor::new()));
    host(&mut ui, &executor);
    let toaster = Toaster::of(&ui.runtime);
    toaster.show(Toast::new("Connected"));
    ui.prepare_frame();
    ui.prepare_frame();
    let key = |ui: &mut Ui, key: Key| {
        ui.dispatch(InputEvent::KeyDown {
            key,
            modifiers: Modifiers::default(),
            repeat: false,
        });
        ui.prepare_frame();
    };
    key(&mut ui, Key::Function(8));
    let card = with_role(&ui, Role::Status)[0];
    assert_eq!(ui.input.focused(), Some(card));
    key(&mut ui, Key::Escape);
    assert!(toaster.showing().is_empty());
}

#[test]
fn past_four_the_oldest_wait() {
    let mut ui = Ui::new(800., 600.);
    let executor = Rc::new(RefCell::new(LocalExecutor::new()));
    host(&mut ui, &executor);
    let toaster = Toaster::of(&ui.runtime);
    for i in 0..6 {
        toaster.show(Toast::new(format!("Toast {i}")).kind(ToastKind::Error));
    }
    ui.prepare_frame();
    ui.prepare_frame();
    assert_eq!(with_role(&ui, Role::Alert).len(), 4);
    assert!(!texts(&ui).contains(&"Toast 0".to_owned()));
}
