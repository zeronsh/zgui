//! A pointer move tells the nodes it leaves and enters before it is
//! routed: their listeners may remove what the move goes to, the node
//! under the pointer or the one capturing it.
use std::{cell::RefCell, rc::Rc};
use zgx::{
    input::{InputDispatcher, InputEvent, NodeInput, PointerButton},
    scene::{Color, Layout, NodeId, NodeKind, Scene, Style},
};

fn setup() -> (Rc<RefCell<Scene>>, InputDispatcher, NodeId, NodeId) {
    let mut scene = Scene::new(200., 100.);
    scene.set_kind(scene.root(), NodeKind::Container(Layout::Row));
    let mut append = || {
        scene.append(
            scene.root(),
            NodeKind::Rect(Color(1, 2, 3, 255)),
            Style {
                width: Some(50.),
                height: Some(50.),
                flex_shrink: 0.,
                ..Default::default()
            },
        )
    };
    let a = append();
    let b = append();
    scene.flush();
    (Rc::new(RefCell::new(scene)), InputDispatcher::new(), a, b)
}

#[test]
fn entering_a_node_may_remove_it_before_the_move_is_routed() {
    let (scene, input, a, _) = setup();
    let s = scene.clone();
    let _a = input.register(a, NodeInput::default(), move |e| {
        if matches!(e.event, InputEvent::PointerEnter) {
            s.borrow_mut().remove(a);
        }
    });
    input.dispatch(&scene, InputEvent::PointerMove { x: 10., y: 10. });
    assert!(!scene.borrow().contains(a));
}

#[test]
fn leaving_a_node_may_remove_the_next_hit_before_move_routing() {
    let (scene, input, a, b) = setup();
    let s = scene.clone();
    let _a = input.register(a, NodeInput::default(), move |e| {
        if matches!(e.event, InputEvent::PointerLeave) {
            s.borrow_mut().remove(b);
        }
    });
    let _b = input.register(b, NodeInput::default(), |_| {});
    input.dispatch(&scene, InputEvent::PointerMove { x: 10., y: 10. });
    input.dispatch(&scene, InputEvent::PointerMove { x: 75., y: 10. });
    assert!(!scene.borrow().contains(b));
}

#[test]
fn enter_callback_may_remove_the_captured_node() {
    let (scene, input, a, b) = setup();
    let _a = input.register(a, NodeInput::default(), |e| {
        if matches!(e.event, InputEvent::PointerDown { .. }) {
            e.capture_pointer();
        }
    });
    let s = scene.clone();
    let _b = input.register(b, NodeInput::default(), move |e| {
        if matches!(e.event, InputEvent::PointerEnter) {
            s.borrow_mut().remove(a);
        }
    });
    input.dispatch(
        &scene,
        InputEvent::PointerDown {
            x: 10.,
            y: 10.,
            button: PointerButton::Primary,
        },
    );
    assert_eq!(input.captured(), Some(a));
    input.dispatch(&scene, InputEvent::PointerMove { x: 75., y: 10. });
    assert!(!scene.borrow().contains(a));
    assert_eq!(input.captured(), None);
}

#[test]
fn the_move_still_reaches_what_survives_its_hover_listeners() {
    let (scene, input, a, b) = setup();
    let moves = Rc::new(std::cell::Cell::new(0));
    let s = scene.clone();
    let _a = input.register(a, NodeInput::default(), move |e| {
        if matches!(e.event, InputEvent::PointerEnter) {
            s.borrow_mut().remove(b);
        }
    });
    let seen = moves.clone();
    let _keep = input.register(a, NodeInput::default(), move |e| {
        if matches!(e.event, InputEvent::PointerMove { .. }) {
            seen.set(seen.get() + 1);
        }
    });
    input.dispatch(&scene, InputEvent::PointerMove { x: 10., y: 10. });
    assert_eq!(moves.get(), 1, "the node entered still hears the move");
}

/// A hover card or tooltip that hides itself as the pointer enters it.
#[test]
fn hiding_a_hovered_view_from_its_enter_handler_does_not_panic() {
    use zgx::{compose::prelude::*, widgets::Ui};
    let mut ui = Ui::new(200., 100.);
    let shown = ui.signal(true);
    let read = shown.clone();
    let write = shown.clone();
    let view = ui.mount(switch(
        move || read.get(),
        move |on, _| {
            if on {
                let write = write.clone();
                div().size(50., 50.).id("hovered").on_event(move |e| {
                    if matches!(e.event, InputEvent::PointerEnter) {
                        write.set(false);
                    }
                })
            } else {
                div()
            }
        },
    ));
    ui.prepare_frame();
    assert!(view.find("hovered").is_some());
    ui.dispatch(InputEvent::PointerMove { x: 10., y: 10. });
    assert!(!shown.get());
    assert!(view.find("hovered").is_none());
}

/// A parent's enter handler that moves the pointer on (a dispatch of its
/// own) leaves no child hovered: the move it made told them as they are.
#[test]
fn late_enter_keeps_hover_style_on_after_pointer_redirect() {
    use std::cell::Cell;
    use zgx::{compose::prelude::*, scene::NodeKind, widgets::Ui};
    let mut ui = Ui::new(200., 100.);
    let input = ui.input.clone();
    let scene = ui.scene.clone();
    let once = Cell::new(true);
    let view = ui.mount(
        row()
            .child(
                column()
                    .size(50., 50.)
                    .on_event(move |e| {
                        if matches!(e.event, InputEvent::PointerEnter) && once.replace(false) {
                            input.dispatch(&scene, InputEvent::PointerMove { x: 75., y: 10. });
                        }
                    })
                    .child(
                        div()
                            .id("a")
                            .size(50., 50.)
                            .bg(rgb(0x112233))
                            .hover(|s| s.bg(rgb(0xff0000)))
                            .on_click(|| {}),
                    ),
            )
            .child(div().id("b").size(50., 50.).on_click(|| {})),
    );
    ui.prepare_frame();
    let a = view.find("a").unwrap();
    let b = view.find("b").unwrap();
    ui.dispatch(InputEvent::PointerMove { x: 10., y: 10. });
    ui.dispatch(InputEvent::PointerMove { x: 75., y: 10. });
    ui.prepare_frame();
    assert_eq!(ui.input.hovered(), Some(b));
    let actual = match ui.scene.borrow().kind(a) {
        NodeKind::Rect(c) => *c,
        NodeKind::Panel { quad, .. } => quad.fill,
        other => panic!("unexpected {other:?}"),
    };
    assert_eq!(actual, rgb(0x112233));
}
