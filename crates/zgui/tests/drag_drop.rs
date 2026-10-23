use std::{cell::RefCell, rc::Rc};
use zgui::{
    compose::prelude::*,
    input::{InputEvent, Key, Modifiers, PointerButton},
    widgets::Ui,
};
#[derive(Clone, Debug, PartialEq)]
struct Card(u32);
fn pointer(ui: &Ui, event: InputEvent) {
    ui.input.dispatch(&ui.scene, event);
    ui.prepare_frame();
}
fn down(ui: &Ui, x: f32, y: f32) {
    pointer(
        ui,
        InputEvent::PointerDown {
            x,
            y,
            button: PointerButton::Primary,
        },
    );
}
fn moved(ui: &Ui, x: f32, y: f32) {
    pointer(ui, InputEvent::PointerMove { x, y });
}
fn up(ui: &Ui, x: f32, y: f32) {
    pointer(
        ui,
        InputEvent::PointerUp {
            x,
            y,
            button: PointerButton::Primary,
        },
    );
}
#[test]
fn typed_drop_preview_and_click_suppression() {
    let mut ui = Ui::new(400., 200.);
    let log = Rc::new(RefCell::new(Vec::new()));
    let dropped = log.clone();
    let ended = log.clone();
    let clicked = log.clone();
    let view = ui.mount(
        row()
            .gap(20.)
            .child(
                button()
                    .child(text("Card"))
                    .id("source")
                    .size(100., 80.)
                    .on_click(move || clicked.borrow_mut().push("click".to_owned()))
                    .on_drag(|| Card(7))
                    .drag_preview(|value: &Card| {
                        div()
                            .id("preview")
                            .size(100., 80.)
                            .child(text(format!("{}", value.0)))
                    })
                    .on_drag_end(move |_: &Card, accepted| {
                        ended.borrow_mut().push(format!("end:{accepted}"))
                    }),
            )
            .child(
                div()
                    .id("destination")
                    .size(100., 80.)
                    .on_drop(move |card: &Card, _| {
                        dropped.borrow_mut().push(format!("drop:{}", card.0))
                    }),
            ),
    );
    ui.prepare_frame();
    let original = ui.scene.borrow().children(ui.root()).len();
    down(&ui, 20., 20.);
    moved(&ui, 22., 20.);
    assert!(!ui.input.is_dragging());
    up(&ui, 22., 20.);
    assert_eq!(&*log.borrow(), &["click"]);
    log.borrow_mut().clear();
    down(&ui, 20., 20.);
    moved(&ui, 35., 20.);
    assert!(ui.input.is_dragging());
    assert_eq!(ui.scene.borrow().children(ui.root()).len(), original + 1);
    moved(&ui, 150., 20.);
    up(&ui, 150., 20.);
    assert_eq!(&*log.borrow(), &["drop:7", "end:true"]);
    assert!(!ui.input.is_dragging());
    assert_eq!(ui.scene.borrow().children(ui.root()).len(), original);
    assert!(view.is_mounted());
}
#[test]
fn rejected_outside_and_escape_drags_end_without_drop() {
    let mut ui = Ui::new(400., 200.);
    let ends = Rc::new(RefCell::new(Vec::new()));
    let log = ends.clone();
    let view = ui.mount(
        row()
            .gap(20.)
            .child(
                div()
                    .id("source")
                    .size(100., 80.)
                    .on_drag(|| Card(7))
                    .on_drag_end(move |_: &Card, accepted| log.borrow_mut().push(accepted)),
            )
            .child(
                div()
                    .size(100., 80.)
                    .on_drop_when(|_: &Card| false, |_: &Card, _| panic!("rejected drop")),
            ),
    );
    ui.prepare_frame();
    down(&ui, 20., 20.);
    moved(&ui, 150., 20.);
    up(&ui, 150., 20.);
    down(&ui, 20., 20.);
    moved(&ui, 300., 150.);
    up(&ui, 300., 150.);
    down(&ui, 20., 20.);
    moved(&ui, 35., 20.);
    pointer(
        &ui,
        InputEvent::KeyDown {
            key: Key::Escape,
            modifiers: Modifiers::default(),
            repeat: false,
        },
    );
    assert_eq!(&*ends.borrow(), &[false, false, false]);
    assert!(!ui.input.is_dragging());
    view.unmount();
}
#[test]
fn removing_source_disposes_preview_and_payload() {
    let mut ui = Ui::new(400., 200.);
    let value = Rc::new(());
    let weak = Rc::downgrade(&value);
    let source = ui.mount(
        div()
            .size(100., 80.)
            .on_drag(move || value.clone())
            .drag_preview(|_: &Rc<()>| div().size(40., 40.)),
    );
    ui.prepare_frame();
    down(&ui, 20., 20.);
    moved(&ui, 35., 20.);
    assert!(ui.input.is_dragging());
    source.unmount();
    ui.prepare_frame();
    assert!(!ui.input.is_dragging());
    assert!(weak.upgrade().is_none());
    assert!(ui.scene.borrow().children(ui.root()).is_empty());
}
#[test]
fn nearest_typed_destination_wins_and_removed_target_cannot_receive_drop() {
    let mut ui = Ui::new(400., 200.);
    let log = Rc::new(RefCell::new(Vec::new()));
    let parent = log.clone();
    let child = log.clone();
    let source = ui.mount(div().size(80., 80.).on_drag(|| Card(1)));
    let target = ui.mount(
        div()
            .absolute()
            .left(120.)
            .size(160., 100.)
            .on_drop(move |_: &Card, _| parent.borrow_mut().push("parent"))
            .child(
                div()
                    .id("inner")
                    .size(80., 80.)
                    .on_drop(move |_: &Card, _| child.borrow_mut().push("child")),
            ),
    );
    ui.prepare_frame();
    down(&ui, 20., 20.);
    moved(&ui, 140., 20.);
    up(&ui, 140., 20.);
    assert_eq!(&*log.borrow(), &["child"]);
    down(&ui, 20., 20.);
    moved(&ui, 140., 20.);
    target.unmount();
    up(&ui, 140., 20.);
    assert_eq!(&*log.borrow(), &["child"]);
    assert!(!ui.input.is_dragging());
    source.unmount();
}
#[test]
fn hidden_source_cancels_and_releases_preview() {
    let mut ui = Ui::new(400., 200.);
    let source = ui.mount(
        div()
            .size(100., 80.)
            .on_drag(|| Card(1))
            .drag_preview(|_: &Card| div().size(20., 20.)),
    );
    ui.prepare_frame();
    down(&ui, 20., 20.);
    moved(&ui, 35., 20.);
    let mut style = ui.scene.borrow().style(source.node());
    style.layout_options = Some(std::sync::Arc::new(zgui::layout::LayoutOptions {
        visible: Some(false),
        ..Default::default()
    }));
    ui.scene.borrow_mut().set_style(source.node(), style);
    ui.prepare_frame();
    assert!(!ui.input.is_dragging());
    assert_eq!(ui.scene.borrow().children(ui.root()).len(), 1);
}
#[test]
fn grouped_external_paths_route_once_to_nearest_handler() {
    let mut ui = Ui::new(300., 200.);
    let received = Rc::new(RefCell::new(Vec::new()));
    let output = received.clone();
    ui.mount(
        div()
            .size(200., 100.)
            .on_files_drop(move |paths, _| output.borrow_mut().push(paths.to_vec())),
    );
    ui.prepare_frame();
    pointer(
        &ui,
        InputEvent::FilesDrop {
            x: 20.,
            y: 20.,
            paths: vec!["/tmp/a".into(), "/tmp/b".into()].into(),
        },
    );
    assert_eq!(
        &*received.borrow(),
        &[vec![
            std::path::PathBuf::from("/tmp/a"),
            std::path::PathBuf::from("/tmp/b")
        ]]
    );
}
#[test]
fn nearest_nested_source_wins_capture() {
    let mut ui = Ui::new(400., 200.);
    let got = Rc::new(RefCell::new(Vec::new()));
    let result = got.clone();
    ui.mount(
        row()
            .gap(20.)
            .child(
                div()
                    .size(100., 80.)
                    .on_drag(|| Card(99))
                    .child(div().size(80., 60.).on_drag(|| Card(1))),
            )
            .child(
                div()
                    .size(100., 80.)
                    .on_drop(move |card: &Card, _| result.borrow_mut().push(card.0)),
            ),
    );
    ui.prepare_frame();
    down(&ui, 20., 20.);
    moved(&ui, 150., 20.);
    up(&ui, 150., 20.);
    assert_eq!(&*got.borrow(), &[1]);
}
#[test]
fn disabled_source_cancellation_releases_preview_and_notifies_end() {
    let mut ui = Ui::new(400., 200.);
    let ends = Rc::new(RefCell::new(Vec::new()));
    let results = ends.clone();
    let source = ui.mount(
        div()
            .size(100., 80.)
            .on_drag(|| Card(1))
            .drag_preview(|_: &Card| div().size(20., 20.))
            .on_drag_end(move |_: &Card, accepted| results.borrow_mut().push(accepted)),
    );
    ui.prepare_frame();
    down(&ui, 20., 20.);
    moved(&ui, 35., 20.);
    ui.set_disabled(source.node(), true);
    ui.prepare_frame();
    assert!(!ui.input.is_dragging());
    assert_eq!(&*ends.borrow(), &[false]);
    assert_eq!(ui.scene.borrow().children(ui.root()).len(), 1);
}
#[test]
fn preview_keeps_grab_point_or_fixed_cursor_offset() {
    for cursor_offset in [None, Some((12., 12.))] {
        let mut ui = Ui::new(400., 300.);
        let source = div().id("source").size(100., 80.).on_drag(|| Card(1));
        let preview = |_: &Card| div().size(100., 80.);
        let source = match cursor_offset {
            None => source.drag_preview(preview),
            Some(offset) => source.drag_preview_at_cursor(offset, preview),
        };
        let view = ui.mount(row().p(30.).child(source));
        ui.prepare_frame();
        let original = ui.scene.borrow().children(ui.root()).len();
        // Press 20,30 inside the source at 30,30, then drag.
        down(&ui, 50., 60.);
        moved(&ui, 100., 90.);
        assert!(ui.input.is_dragging());
        let scene = ui.scene.borrow();
        let children = scene.children(ui.root());
        assert_eq!(children.len(), original + 1);
        let bounds = scene.bounds(children[original]);
        let expected = match cursor_offset {
            None => (80., 60.),
            Some(_) => (112., 102.),
        };
        assert_eq!((bounds.x, bounds.y), expected);
        drop(scene);
        up(&ui, 100., 90.);
        view.unmount();
    }
}

#[test]
fn overlays_receive_drops_over_their_children() {
    let mut ui = Ui::new(400., 300.);
    let log = Rc::new(RefCell::new(Vec::new()));
    let dropped = log.clone();
    let view = ui.mount(
        column()
            .child(div().id("source").size(40., 40.).on_drag(|| Card(3)))
            .child(
                overlay()
                    .id("area")
                    .size(200., 200.)
                    .on_drop(move |card: &Card, _| dropped.borrow_mut().push(card.0))
                    .child(div().id("child").size(100., 100.)),
            ),
    );
    ui.prepare_frame();
    let bounds = |id: &str| ui.scene.borrow().bounds(view.find(id).unwrap());
    let (source, child) = (bounds("source"), bounds("child"));
    down(&ui, source.x + 5., source.y + 5.);
    moved(&ui, source.x + 15., source.y + 15.);
    moved(&ui, child.x + 20., child.y + 20.);
    up(&ui, child.x + 20., child.y + 20.);
    assert_eq!(*log.borrow(), vec![3]);
}

#[test]
fn a_source_the_drop_removes_still_hears_the_drag_end() {
    let mut ui = Ui::new(400., 200.);
    let log = Rc::new(RefCell::new(Vec::new()));
    let here = ui.signal(true);
    let (ended, gone) = (log.clone(), here.clone());
    let view = ui.mount(
        row()
            .child(switch(
                move || here.get(),
                move |here, _| {
                    if !here {
                        return div();
                    }
                    let ended = ended.clone();
                    div()
                        .id("source")
                        .size(100., 80.)
                        .on_drag(|| Card(3))
                        .on_drag_end(move |_: &Card, accepted| {
                            ended.borrow_mut().push(format!("end:{accepted}"))
                        })
                },
            ))
            .child(
                div()
                    .id("destination")
                    .size(100., 80.)
                    // Taking the card moves it here: the source goes.
                    .on_drop(move |_: &Card, _| {
                        gone.set(false);
                    }),
            ),
    );
    ui.prepare_frame();
    assert!(view.find("source").is_some());
    down(&ui, 20., 20.);
    moved(&ui, 60., 20.);
    moved(&ui, 150., 20.);
    assert!(ui.input.is_dragging());
    up(&ui, 150., 20.);
    assert!(view.find("source").is_none());
    assert!(!ui.input.is_dragging());
    assert_eq!(&*log.borrow(), &["end:true"]);
}
#[test]
fn drops_land_under_a_pointer_transparent_layer() {
    // A layer over the destination that lets the pointer through, inside a
    // parent with listeners of its own: the drop still reaches the
    // destination, as a press would.
    let mut ui = Ui::new(400., 200.);
    let log = Rc::new(RefCell::new(Vec::new()));
    let dropped = log.clone();
    ui.mount(
        row()
            .gap(20.)
            .child(div().id("source").size(100., 80.).on_drag(|| Card(7)))
            .child(
                overlay()
                    .size(100., 80.)
                    .on_click(|| {})
                    .child(div().id("destination").size(100., 80.).on_drop(
                        move |card: &Card, _| dropped.borrow_mut().push(format!("drop:{}", card.0)),
                    ))
                    .child(div().id("layer").size(100., 80.).pointer_events(false)),
            ),
    );
    ui.prepare_frame();
    down(&ui, 20., 20.);
    moved(&ui, 35., 20.);
    assert!(ui.input.is_dragging());
    moved(&ui, 150., 20.);
    up(&ui, 150., 20.);
    assert_eq!(&*log.borrow(), &["drop:7"]);
}

/// What a drag carries to other apps (`drag_export`) is the host's to read
/// as it starts; a drag another app took ends accepted at its source
/// (`end_drag_elsewhere`), and its host learns how it ended.
#[test]
fn drags_carry_their_export_and_end_where_the_host_says() {
    use zgui::input::DragItem;
    let mut ui = Ui::new(300., 200.);
    let log = Rc::new(RefCell::new(Vec::new()));
    let ended = log.clone();
    ui.mount(
        div()
            .id("source")
            .size(100., 40.)
            .on_drag(|| Card(5))
            .drag_export(|card: &Card| vec![DragItem::Text(format!("card {}", card.0))])
            .on_drag_end(move |card: &Card, accepted| {
                ended
                    .borrow_mut()
                    .push(format!("end:{}:{accepted}", card.0))
            }),
    );
    ui.prepare_frame();
    down(&ui, 10., 10.);
    moved(&ui, 30., 10.);
    let drag = ui.input.drag_state().expect("dragging");
    assert_eq!(
        drag.export.as_deref(),
        Some(&[DragItem::Text("card 5".into())][..])
    );
    // Out of the window, another app took it.
    moved(&ui, 400., 10.);
    ui.input.end_drag_elsewhere(&ui.scene, true);
    assert!(!ui.input.is_dragging());
    assert_eq!(*log.borrow(), ["end:5:true"]);
    let ended = ui.input.take_drag_ended().expect("ended");
    assert!(ended.accepted);
    assert!(ui.input.take_drag_ended().is_none());
    // The release the platform's session took: nothing more happens.
    up(&ui, 400., 10.);
    assert_eq!(*log.borrow(), ["end:5:true"]);
}
