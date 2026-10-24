use std::{cell::Cell, rc::Rc};
use zgx::{
    compose::prelude::*,
    input::{InputEvent, Key, Modifiers, PointerButton},
    scene::NodeId,
    widgets::Ui,
};

fn panel(ui: &Ui, body: NodeId) -> NodeId {
    ui.scene.borrow().parent(body).unwrap()
}

#[test]
fn popover_follows_anchor_transforms_flips_and_clamps_after_resize() {
    let mut ui = Ui::new(300., 200.);
    let open = ui.signal(false);
    let movement = ui.signal((0., 0.));
    let read_movement = movement.clone();
    let view = ui.mount(
        column().p(20.).child(
            popover(
                "Details",
                open.clone(),
                button()
                    .id("anchor")
                    .size(40., 20.)
                    .p(0.)
                    .child(text("Open"))
                    .reactive_style(move || {
                        let (x, y) = read_movement.get();
                        Styles::new().translate(x, y)
                    }),
            )
            .size(100., 60.)
            .p(0.)
            .child(button().id("body").size(60., 20.).child(text("Inside"))),
        ),
    );
    let anchor = view.find("anchor").unwrap();
    let body = view.find("body").unwrap();
    let panel = panel(&ui, body);
    assert!(ui.input.focus(&ui.scene, Some(anchor)));
    open.set(true);
    ui.prepare_frame();
    let bounds = ui.scene.borrow().bounds(panel);
    assert_eq!(
        (bounds.x, bounds.y, bounds.width, bounds.height),
        (20., 44., 100., 60.)
    );
    movement.set((250., 150.));
    ui.prepare_frame();
    let bounds = ui.scene.borrow().bounds(panel);
    assert_eq!((bounds.x, bounds.y), (200., 106.));
    ui.scene.borrow_mut().resize(200., 140.);
    ui.prepare_frame();
    let bounds = ui.scene.borrow().bounds(panel);
    assert_eq!((bounds.x, bounds.y), (100., 80.));
    view.unmount();
    assert!(ui.scene.borrow().children(ui.root()).is_empty());
}

#[test]
fn popover_dismissal_restores_focus_and_keeps_panel_translation_additive() {
    let mut ui = Ui::new(300., 200.);
    let open = ui.signal(false);
    let view = ui.mount(
        column().p(20.).child(
            popover(
                "Details",
                open.clone(),
                button()
                    .id("anchor")
                    .size(40., 20.)
                    .p(0.)
                    .child(text("Open")),
            )
            .size(100., 60.)
            .p(0.)
            .translate(3., 5.)
            .child(button().id("body").size(60., 20.).child(text("Inside"))),
        ),
    );
    let anchor = view.find("anchor").unwrap();
    let body = view.find("body").unwrap();
    let panel = panel(&ui, body);
    ui.input.focus(&ui.scene, Some(anchor));
    open.set(true);
    ui.prepare_frame();
    assert_eq!(
        (
            ui.scene.borrow().bounds(panel).x,
            ui.scene.borrow().bounds(panel).y
        ),
        (23., 49.)
    );
    assert!(ui.input.focus_scope().is_some());
    ui.dispatch(InputEvent::KeyDown {
        key: Key::Escape,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    assert!(!open.get());
    assert_eq!(ui.input.focused(), Some(anchor));
    open.set(true);
    ui.prepare_frame();
    // Empty panel area must not behave like the outside dismissal surface.
    ui.dispatch(InputEvent::PointerDown {
        x: 110.,
        y: 100.,
        button: PointerButton::Primary,
    });
    assert!(open.get());
    ui.dispatch(InputEvent::PointerDown {
        x: 280.,
        y: 180.,
        button: PointerButton::Primary,
    });
    assert!(!open.get());
    assert_eq!(ui.input.focused(), Some(anchor));
}

struct DropCount(Rc<Cell<usize>>);
impl Drop for DropCount {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}
#[test]
fn popover_body_inherits_lexical_services_and_disposes_with_owner() {
    let mut ui = Ui::new(300., 200.);
    let open = ui.signal(true);
    let drops = Rc::new(Cell::new(0));
    let retained = drops.clone();
    let view = ui.mount(provide(
        String::from("provider"),
        popover("Details", open.clone(), button().child(text("Open")))
            .size(120., 80.)
            .child(component(move |cx| {
                cx.retain(DropCount(retained));
                text(cx.service::<String>().as_str()).id("body")
            })),
    ));
    ui.prepare_frame();
    let body = view.find("body").unwrap();
    assert!(ui.scene.borrow().contains(body));
    assert!(ui.input.focus_scope().is_some());
    view.unmount();
    assert_eq!(drops.get(), 1);
    assert!(ui.input.focus_scope().is_none());
    assert!(ui.scene.borrow().children(ui.root()).is_empty());
    open.set(false);
    open.set(true);
    assert_eq!(drops.get(), 1);
    assert!(ui.scene.borrow().children(ui.root()).is_empty());
}

#[test]
fn open_popover_tracks_anchor_scrolling_without_inheriting_viewport_clip() {
    let mut ui = Ui::new(300., 240.);
    let open = ui.signal(false);
    let offset = ui.signal(0.);
    let view = ui.mount(
        column().p(20.).child(
            scroll(offset.clone()).size(160., 80.).child(
                column()
                    .child(div().h(100.))
                    .child(
                        popover(
                            "Details",
                            open.clone(),
                            button()
                                .id("anchor")
                                .size(60., 20.)
                                .p(0.)
                                .child(text("Open")),
                        )
                        .size(100., 60.)
                        .p(0.)
                        .child(text("Outside clip").id("body")),
                    )
                    .child(div().h(100.)),
            ),
        ),
    );
    ui.prepare_frame();
    offset.set(80.);
    open.set(true);
    ui.prepare_frame();
    let body = view.find("body").unwrap();
    let panel = panel(&ui, body);
    assert_eq!(ui.scene.borrow().bounds(panel).y, 64.);
    offset.set(100.);
    ui.prepare_frame();
    assert_eq!(ui.scene.borrow().bounds(panel).y, 44.);
    let anchor = view.find("anchor").unwrap();
    assert!(!ui.scene.borrow().ancestors(body).any(|id| id == anchor));
    view.unmount();
    assert!(ui.scene.borrow().children(ui.root()).is_empty());
}

#[test]
fn virtual_row_disposal_removes_open_popover_scope_and_subscriptions() {
    let mut ui = Ui::new(400., 300.);
    let offset = ui.signal(0.);
    let open = ui.signal(false);
    let label = ui.signal(String::from("row details"));
    let builds = Rc::new(Cell::new(0));
    let drops = Rc::new(Cell::new(0));
    let reads = Rc::new(Cell::new(0));
    let view = ui.mount(
        virtual_list(offset.clone(), 20., 1, || 1_000_000, |index| index, {
            let open = open.clone();
            let label = label.clone();
            let builds = builds.clone();
            let drops = drops.clone();
            let reads = reads.clone();
            move |_, index, cx| {
                builds.set(builds.get() + 1);
                cx.retain(DropCount(drops.clone()));
                if index != 0 {
                    return text(format!("row {index}"));
                }
                let value = label.clone();
                let reads = reads.clone();
                popover(
                    "Row details",
                    open.clone(),
                    button()
                        .id("virtual-anchor")
                        .size(120., 20.)
                        .p(0.)
                        .child(text("Open details")),
                )
                .id("virtual-popup")
                .size(160., 60.)
                .p(0.)
                .child(
                    text_signal(move || {
                        reads.set(reads.get() + 1);
                        value.get()
                    })
                    .id("virtual-popup-label"),
                )
            }
        })
        .size(220., 100.),
    );
    ui.prepare_frame();
    let anchor = view.find("virtual-anchor").unwrap();
    let popup = view.find("virtual-popup").unwrap();
    let popup_label = view.find("virtual-popup-label").unwrap();
    ui.input.focus(&ui.scene, Some(anchor));
    open.set(true);
    ui.prepare_frame();
    let scope = ui
        .input
        .focus_scope()
        .expect("popover entered its focus scope");
    assert!(ui.scene.borrow().ancestors(popup).any(|node| node == scope));
    assert!(builds.get() - drops.get() <= 7);

    offset.set(1_000.);
    ui.prepare_frame();
    assert!(!ui.scene.borrow().contains(anchor));
    assert!(!ui.scene.borrow().contains(popup));
    assert!(!ui.scene.borrow().contains(scope));
    assert!(!ui.input.has_listeners(popup));
    assert!(ui.input.focus_scope().is_none());
    assert!(
        ui.input
            .focused()
            .is_none_or(|node| ui.scene.borrow().contains(node))
    );
    assert!(view.find("virtual-popup").is_none());
    assert!(ui.semantics.borrow().get(popup_label).is_none());
    assert!(builds.get() - drops.get() <= 7);
    assert!(
        builds.get() <= 14,
        "jumping must not materialize skipped rows"
    );
    let last_reads = reads.get();
    label.set("after disposal".into());
    open.set(true);
    ui.prepare_frame();
    assert_eq!(
        reads.get(),
        last_reads,
        "disposed body has no model subscription"
    );
    assert_eq!(
        ui.scene.borrow().children(ui.root()).len(),
        1,
        "no orphan portal survives the virtual row"
    );
    view.unmount();
    assert_eq!(builds.get(), drops.get());
    assert!(ui.scene.borrow().children(ui.root()).is_empty());
}
