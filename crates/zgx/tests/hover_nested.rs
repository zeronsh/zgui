//! A hover listener that dispatches (moves the pointer on, or back) tells
//! the nodes it leaves and enters itself: the move whose listener it is
//! tells no more of its own, stale, leaves and enters after it.
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use zgx::{
    input::{InputDispatcher, InputEvent, NodeInput},
    scene::{Color, Layout, NodeId, NodeKind, Scene, Style},
};

/// A row of a container (holding `child`) and `b`, each 50 points wide.
fn row() -> (Rc<RefCell<Scene>>, NodeId, NodeId, NodeId) {
    let mut s = Scene::new(200., 100.);
    let root = s.root();
    s.set_kind(root, NodeKind::Container(Layout::Row));
    let style = Style {
        width: Some(50.),
        height: Some(50.),
        flex_shrink: 0.,
        ..Default::default()
    };
    let a = s.append(root, NodeKind::Container(Layout::Column), style.clone());
    let child = s.append(a, NodeKind::Rect(Color(1, 2, 3, 255)), style.clone());
    let b = s.append(root, NodeKind::Rect(Color(1, 2, 3, 255)), style);
    s.flush();
    (Rc::new(RefCell::new(s)), a, child, b)
}

#[test]
fn enter_handler_redirect_cannot_leave_old_child_hovered() {
    let (scene, a, child, b) = row();
    let input = InputDispatcher::new();
    let redirect = Rc::new(Cell::new(true));
    let d = input.clone();
    let ss = scene.clone();
    let _a = input.register(a, NodeInput::default(), move |e| {
        if matches!(e.event, InputEvent::PointerEnter) && redirect.replace(false) {
            d.dispatch(&ss, InputEvent::PointerMove { x: 75., y: 10. });
        }
    });
    let hovered = Rc::new(Cell::new(false));
    let h = hovered.clone();
    let events = Rc::new(RefCell::new(Vec::new()));
    let es = events.clone();
    let _child = input.register(child, NodeInput::default(), move |e| match e.event {
        InputEvent::PointerEnter => {
            h.set(true);
            es.borrow_mut().push("enter");
        }
        InputEvent::PointerLeave => {
            h.set(false);
            es.borrow_mut().push("leave");
        }
        _ => {}
    });
    let _b = input.register(b, NodeInput::default(), |_| {});
    input.dispatch(&scene, InputEvent::PointerMove { x: 10., y: 10. });
    input.dispatch(&scene, InputEvent::PointerMove { x: 75., y: 10. });
    assert_eq!(input.hovered(), Some(b));
    assert!(
        !hovered.get(),
        "old child remains hovered after two moves to sibling: {:?}",
        events.borrow()
    );
}

#[test]
fn leave_handler_redirect_cannot_clear_new_parent_hover() {
    let (scene, a, child, b) = row();
    let input = InputDispatcher::new();
    let hovered = Rc::new(Cell::new(false));
    let h = hovered.clone();
    let events = Rc::new(RefCell::new(Vec::new()));
    let es = events.clone();
    let _a = input.register(a, NodeInput::default(), move |e| match e.event {
        InputEvent::PointerEnter => {
            h.set(true);
            es.borrow_mut().push("enter");
        }
        InputEvent::PointerLeave => {
            h.set(false);
            es.borrow_mut().push("leave");
        }
        _ => {}
    });
    let redirect = Rc::new(Cell::new(true));
    let d = input.clone();
    let ss = scene.clone();
    let _child = input.register(child, NodeInput::default(), move |e| {
        if matches!(e.event, InputEvent::PointerLeave) && redirect.replace(false) {
            d.dispatch(&ss, InputEvent::PointerMove { x: 10., y: 10. });
        }
    });
    let _b = input.register(b, NodeInput::default(), |_| {});
    input.dispatch(&scene, InputEvent::PointerMove { x: 10., y: 10. });
    assert!(hovered.get());
    input.dispatch(&scene, InputEvent::PointerMove { x: 75., y: 10. });
    input.dispatch(&scene, InputEvent::PointerMove { x: 10., y: 10. });
    assert_eq!(input.hovered(), Some(child));
    assert!(
        hovered.get(),
        "newly reentered parent ends unhovered: {:?}",
        events.borrow()
    );
}

#[test]
fn later_enter_listener_cannot_overwrite_a_nested_leave() {
    let mut s = Scene::new(200., 100.);
    let root = s.root();
    s.set_kind(root, NodeKind::Container(Layout::Row));
    let style = Style {
        width: Some(50.),
        height: Some(50.),
        flex_shrink: 0.,
        ..Default::default()
    };
    let a = s.append(root, NodeKind::Rect(Color(1, 2, 3, 255)), style.clone());
    let b = s.append(root, NodeKind::Rect(Color(1, 2, 3, 255)), style);
    s.flush();
    let scene = Rc::new(RefCell::new(s));
    let input = InputDispatcher::new();
    let once = Cell::new(true);
    let d = input.clone();
    let ss = scene.clone();
    let _first = input.register(a, NodeInput::default(), move |e| {
        if matches!(e.event, InputEvent::PointerEnter) && once.replace(false) {
            d.dispatch(&ss, InputEvent::PointerMove { x: 75., y: 10. });
        }
    });
    let hovered = Rc::new(Cell::new(false));
    let h = hovered.clone();
    let _second = input.listen(a, move |e| match e.event {
        InputEvent::PointerEnter => h.set(true),
        InputEvent::PointerLeave => h.set(false),
        _ => {}
    });
    let _b = input.register(b, NodeInput::default(), |_| {});
    input.dispatch(&scene, InputEvent::PointerMove { x: 10., y: 10. });
    input.dispatch(&scene, InputEvent::PointerMove { x: 75., y: 10. });
    assert_eq!(input.hovered(), Some(b));
    assert!(
        !hovered.get(),
        "late listener re-enters old target after it left"
    );
}

#[test]
fn ordinary_enter_then_leave_control() {
    let (scene, a, child, b) = row();
    let input = InputDispatcher::new();
    let hovered = Rc::new(Cell::new(false));
    let h = hovered.clone();
    let _a = input.register(a, NodeInput::default(), move |e| match e.event {
        InputEvent::PointerEnter => h.set(true),
        InputEvent::PointerLeave => h.set(false),
        _ => {}
    });
    let _child = input.register(child, NodeInput::default(), |_| {});
    let _b = input.register(b, NodeInput::default(), |_| {});
    input.dispatch(&scene, InputEvent::PointerMove { x: 10., y: 10. });
    assert!(hovered.get());
    input.dispatch(&scene, InputEvent::PointerMove { x: 75., y: 10. });
    assert_eq!(input.hovered(), Some(b));
    assert!(!hovered.get());
}

/// A child's leave that moves the pointer on to a third node: its parent,
/// still to be left by the outer move, is left all the same.
#[test]
fn redirecting_leave_to_third_target_still_leaves_old_parent() {
    let mut s = Scene::new(200., 100.);
    let root = s.root();
    s.set_kind(root, NodeKind::Container(Layout::Row));
    let style = Style {
        width: Some(50.),
        height: Some(50.),
        flex_shrink: 0.,
        ..Default::default()
    };
    let a = s.append(root, NodeKind::Container(Layout::Column), style.clone());
    let child = s.append(a, NodeKind::Rect(Color(1, 2, 3, 255)), style.clone());
    let b = s.append(root, NodeKind::Rect(Color(1, 2, 3, 255)), style.clone());
    let c = s.append(root, NodeKind::Rect(Color(1, 2, 3, 255)), style);
    s.flush();
    let scene = Rc::new(RefCell::new(s));
    let input = InputDispatcher::new();
    let hovered = Rc::new(Cell::new(false));
    let h = hovered.clone();
    let events = Rc::new(RefCell::new(Vec::new()));
    let es = events.clone();
    let _a = input.register(a, NodeInput::default(), move |e| match e.event {
        InputEvent::PointerEnter => {
            h.set(true);
            es.borrow_mut().push("enter");
        }
        InputEvent::PointerLeave => {
            h.set(false);
            es.borrow_mut().push("leave");
        }
        _ => {}
    });
    let redirect = Rc::new(Cell::new(true));
    let d = input.clone();
    let ss = scene.clone();
    let _child = input.register(child, NodeInput::default(), move |e| {
        if matches!(e.event, InputEvent::PointerLeave) && redirect.replace(false) {
            d.dispatch(&ss, InputEvent::PointerMove { x: 125., y: 10. });
        }
    });
    let _b = input.register(b, NodeInput::default(), |_| {});
    let _c = input.register(c, NodeInput::default(), |_| {});
    input.dispatch(&scene, InputEvent::PointerMove { x: 10., y: 10. });
    assert!(hovered.get());
    input.dispatch(&scene, InputEvent::PointerMove { x: 75., y: 10. });
    input.dispatch(&scene, InputEvent::PointerMove { x: 125., y: 10. });
    assert_eq!(input.hovered(), Some(c));
    assert!(
        !hovered.get(),
        "parent never receives its pending leave: {:?}",
        events.borrow()
    );
}

/// Random rows of parents and children whose listeners move the pointer
/// elsewhere as they are entered or left, now and then: after every move
/// the last a node's listener heard is an enter exactly while the node is
/// on the hovered path. (A listener after one that moved the pointer off
/// as the node was entered hears only the leave: entered and left in one.)
#[test]
fn redirecting_listeners_leave_every_node_told_as_it_is() {
    struct Rng(u64);
    impl Rng {
        fn below(&mut self, n: u64) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0 % n
        }
    }
    for seed in 1..=200u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let mut s = Scene::new(200., 100.);
        let root = s.root();
        s.set_kind(root, NodeKind::Container(Layout::Row));
        let style = Style {
            width: Some(50.),
            height: Some(50.),
            flex_shrink: 0.,
            ..Default::default()
        };
        let mut nodes = Vec::new();
        for _ in 0..4 {
            let parent = s.append(root, NodeKind::Container(Layout::Column), style.clone());
            let child = s.append(parent, NodeKind::Rect(Color(1, 2, 3, 255)), style.clone());
            nodes.extend([parent, child]);
        }
        s.flush();
        let scene = Rc::new(RefCell::new(s));
        let input = InputDispatcher::new();
        let entered: Vec<Rc<Cell<bool>>> = nodes.iter().map(|_| Rc::default()).collect();
        let mut keep = Vec::new();
        for (index, &node) in nodes.iter().enumerate() {
            // Counted by a listener of its own: one that dispatches is not
            // called again inside its own call (it is borrowed).
            let told = entered[index].clone();
            let count = move |e: &mut zgx::input::EventContext| match e.event {
                InputEvent::PointerEnter => told.set(true),
                InputEvent::PointerLeave => told.set(false),
                _ => {}
            };
            // Some redirect once, as they are entered or left, to a column.
            let redirect = (rng.below(3) == 0).then(|| {
                (
                    rng.below(2) == 0,
                    Cell::new(true),
                    10. + rng.below(5) as f32 * 50.,
                )
            });
            let (d, ss) = (input.clone(), scene.clone());
            let redirecting = move |e: &mut zgx::input::EventContext| {
                let entering = match e.event {
                    InputEvent::PointerEnter => true,
                    InputEvent::PointerLeave => false,
                    _ => return,
                };
                if let Some((on_enter, once, x)) = &redirect
                    && *on_enter == entering
                    && once.replace(false)
                {
                    d.dispatch(&ss, InputEvent::PointerMove { x: *x, y: 10. });
                }
            };
            // Either order: a later listener must not hear a stale event.
            if rng.below(2) == 0 {
                keep.push(input.register(node, NodeInput::default(), redirecting));
                keep.push(input.listen(node, count));
            } else {
                keep.push(input.register(node, NodeInput::default(), count));
                keep.push(input.listen(node, redirecting));
            }
        }
        for step in 0..12 {
            let x = 10. + rng.below(5) as f32 * 50.;
            input.dispatch(&scene, InputEvent::PointerMove { x, y: 10. });
            let hovered = input.hovered();
            let scene = scene.borrow();
            for (index, &node) in nodes.iter().enumerate() {
                let on_path = hovered.is_some_and(|hit| {
                    let mut at = Some(hit);
                    while let Some(id) = at {
                        if id == node {
                            return true;
                        }
                        at = scene.parent(id);
                    }
                    false
                });
                assert_eq!(
                    entered[index].get(),
                    on_path,
                    "seed {seed} step {step}: node {index} told wrongly (hovered {hovered:?})"
                );
            }
        }
    }
}

/// Research A's oracle: every first position, outer move and redirect
/// target, a redirecting listener on every node, on enter or on leave,
/// before or after the node's observer. After the pointer settles, every
/// observer says hovered exactly for the nodes the pointer is inside.
#[test]
fn every_redirect_leaves_observers_as_the_pointer_is() {
    let points = [-10., 10., 75., 125.];
    let mut failures = Vec::new();
    for initial in 0..4 {
        for outer in 0..4 {
            for redirected in 0..4 {
                for interceptor in 0..7 {
                    for entering in [false, true] {
                        for first in [false, true] {
                            let mut s = Scene::new(200., 100.);
                            let root = s.root();
                            s.set_kind(root, NodeKind::Container(Layout::Row));
                            let style = Style {
                                width: Some(50.),
                                height: Some(50.),
                                flex_shrink: 0.,
                                ..Default::default()
                            };
                            let mut ids = vec![root];
                            for _ in 0..3 {
                                let parent = s.append(
                                    root,
                                    NodeKind::Container(Layout::Column),
                                    style.clone(),
                                );
                                let child = s.append(
                                    parent,
                                    NodeKind::Rect(Color(1, 2, 3, 255)),
                                    style.clone(),
                                );
                                ids.extend([parent, child]);
                            }
                            s.flush();
                            let scene = Rc::new(RefCell::new(s));
                            let input = InputDispatcher::new();
                            let states: Vec<_> =
                                ids.iter().map(|_| Rc::new(Cell::new(false))).collect();
                            let mut bindings = Vec::new();
                            for (&id, state) in ids.iter().zip(&states) {
                                let h = state.clone();
                                bindings.push(input.register(id, NodeInput::default(), move |e| {
                                    match e.event {
                                        InputEvent::PointerEnter => h.set(true),
                                        InputEvent::PointerLeave => h.set(false),
                                        _ => {}
                                    }
                                }));
                            }
                            let armed = Rc::new(Cell::new(false));
                            let fired = Rc::new(Cell::new(false));
                            let (a, f, d, ss) =
                                (armed.clone(), fired.clone(), input.clone(), scene.clone());
                            let redirect_x = points[redirected];
                            let callback = move |e: &mut zgx::input::EventContext| {
                                let matching = if entering {
                                    matches!(e.event, InputEvent::PointerEnter)
                                } else {
                                    matches!(e.event, InputEvent::PointerLeave)
                                };
                                if matching && a.get() && !f.replace(true) {
                                    d.dispatch(
                                        &ss,
                                        InputEvent::PointerMove {
                                            x: redirect_x,
                                            y: 10.,
                                        },
                                    );
                                }
                            };
                            bindings.push(if first {
                                input.listen_first(ids[interceptor], callback)
                            } else {
                                input.listen(ids[interceptor], callback)
                            });
                            input.dispatch(
                                &scene,
                                InputEvent::PointerMove {
                                    x: points[initial],
                                    y: 10.,
                                },
                            );
                            armed.set(true);
                            input.dispatch(
                                &scene,
                                InputEvent::PointerMove {
                                    x: points[outer],
                                    y: 10.,
                                },
                            );
                            let settled = if fired.get() { redirected } else { outer };
                            input.dispatch(
                                &scene,
                                InputEvent::PointerMove {
                                    x: points[settled],
                                    y: 10.,
                                },
                            );
                            let expected = (settled != 0).then(|| ids[settled * 2]);
                            let mut ancestry = Vec::new();
                            let mut current = expected;
                            while let Some(id) = current {
                                ancestry.push(id);
                                current = scene.borrow().parent(id);
                            }
                            let wrong: Vec<_> = ids
                                .iter()
                                .enumerate()
                                .filter(|(i, id)| states[*i].get() != ancestry.contains(id))
                                .map(|(i, _)| i)
                                .collect();
                            if input.hovered() != expected || !wrong.is_empty() {
                                failures.push(format!(
                                    "initial={initial} outer={outer} redirected={redirected} \
                                     interceptor={interceptor} entering={entering} first={first} \
                                     wrong={wrong:?}"
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of 1792 wrong, first: {:#?}",
        failures.len(),
        &failures[..failures.len().min(10)]
    );
}
