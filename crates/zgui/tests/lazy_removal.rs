//! A large subtree (`Ui::remove` of more than a few hundred nodes) leaves
//! at once and is freed over the frames after: from the moment it goes,
//! nothing reaches its nodes (hit testing, accessibility, focus, keys,
//! selection), its effects never run, and its components' resources drop.
use std::{cell::Cell, rc::Rc};
use zgui::{
    actions::{Action, KeyBinding, Keymap, Keystroke},
    compose::prelude::*,
    input::{InputEvent, Key, Modifiers, PointerButton},
    scene::NodeId,
    widgets::Ui,
};

#[derive(Debug)]
struct Shortcut;

/// Counts what lives: dropping one counts it gone.
struct Lease(Rc<Cell<usize>>);
impl Lease {
    fn new(live: &Rc<Cell<usize>>) -> Self {
        live.set(live.get() + 1);
        Self(live.clone())
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        self.0.set(self.0.get() - 1);
    }
}

fn key(key: Key) -> InputEvent {
    InputEvent::KeyDown {
        key,
        modifiers: Modifiers::default(),
        repeat: false,
    }
}
fn click(ui: &mut Ui, x: f32, y: f32) {
    let button = PointerButton::Primary;
    ui.dispatch(InputEvent::PointerDown { x, y, button });
    ui.dispatch(InputEvent::PointerUp { x, y, button });
}
/// Every node under `root`, `root` included.
fn subtree(ui: &Ui, root: NodeId) -> Vec<NodeId> {
    let scene = ui.scene.borrow();
    let mut ids = vec![root];
    let mut at = 0;
    while at < ids.len() {
        ids.extend_from_slice(scene.children(ids[at]));
        at += 1;
    }
    ids
}

/// `rows` rows of a label and a button, selectable, the page taking a key.
fn page(rows: usize, clicks: &Rc<Cell<usize>>, keys: &Rc<Cell<usize>>) -> View {
    let (clicks, keys) = (clicks.clone(), keys.clone());
    column()
        .id("page")
        .w(400.)
        .text_size(10.)
        .line_height(20.)
        .selectable(true)
        .keymap(
            Keymap::new().bind(
                KeyBinding::new(
                    [Keystroke::new(Key::Function(5), Modifiers::default())],
                    Action::new(Shortcut),
                )
                .unwrap(),
            ),
        )
        .on_action(move |_: &Shortcut, cx| {
            keys.set(keys.get() + 1);
            cx.stop_propagation();
        })
        .children((0..rows).map(move |i| {
            let clicks = clicks.clone();
            row().h(20.).child(text(format!("Row {i}"))).child(
                button()
                    .id(if i == 0 { "first" } else { "button" })
                    .w(60.)
                    .h(20.)
                    .on_click(move || clicks.set(clicks.get() + 1))
                    .child(text("Go")),
            )
        }))
}

#[test]
fn a_removed_page_is_out_of_reach_from_the_frame_it_goes() {
    let mut ui = Ui::new(800., 600.);
    let (clicks, keys) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let outside = ui.mount(button().id("outside").w(60.).h(20.));
    let mounted = ui.mount(page(300, &clicks, &keys));
    ui.prepare_frame();
    let root = mounted.find("page").unwrap();
    let first = mounted.find("first").unwrap();
    let ids = subtree(&ui, root);
    assert!(ids.len() > 1000, "a page large enough to go lazily");
    // Focus in it, the pointer over it, text in it selected.
    let at = ui.scene.borrow().bounds(first);
    let (x, y) = (at.x + 4., at.y + 4.);
    ui.input.focus(&ui.scene, Some(first));
    ui.dispatch(InputEvent::PointerMove { x, y });
    ui.dispatch(key(Key::Function(5)));
    assert_eq!(keys.get(), 1, "its shortcut works while it is there");
    let label = ui.scene.borrow().bounds(ids[2]);
    let (lx, ly) = (label.x + 1., label.y + 10.);
    let button = PointerButton::Primary;
    ui.dispatch(InputEvent::PointerDown {
        x: lx,
        y: ly,
        button,
    });
    ui.dispatch(InputEvent::PointerMove { x: lx + 30., y: ly });
    ui.dispatch(InputEvent::PointerUp {
        x: lx + 30.,
        y: ly,
        button,
    });
    assert!(ui.has_text_selection(), "text in it selected");
    let nodes = ui.scene.borrow().len();

    mounted.unmount();
    // Before any frame: gone from the scene, freed later.
    assert!(ui.reclaiming());
    assert_eq!(ui.scene.borrow().len(), nodes - ids.len());
    {
        let scene = ui.scene.borrow();
        assert!(ids.iter().all(|id| !scene.contains(*id)));
        assert!(scene.hit_test_all(x, y).iter().all(|id| !ids.contains(id)));
    }
    // No accessibility information of it is read.
    {
        let semantics = ui.semantics.borrow();
        assert!(ids.iter().all(|id| semantics.get(*id).is_none()));
        assert!(ids.iter().all(|id| semantics.node_revision(*id).is_none()));
        assert!(semantics.iter().all(|(id, _)| !ids.contains(&id)));
    }
    // Neither focus nor the pointer's hover names it.
    assert!(ui.input.focused().is_none_or(|id| !ids.contains(&id)));
    assert!(ui.input.hovered().is_none_or(|id| !ids.contains(&id)));
    // Its shortcut, its buttons and its text are out of reach.
    ui.dispatch(key(Key::Function(5)));
    click(&mut ui, x, y);
    assert_eq!((keys.get(), clicks.get()), (1, 0));
    assert_eq!(ui.selected_text(), "");
    ui.prepare_frame();
    assert!(ui.input.focused().is_none_or(|id| !ids.contains(&id)));
    // What stays still works.
    let kept = outside.find("outside").unwrap();
    ui.input.focus(&ui.scene, Some(kept));
    assert_eq!(ui.input.focused(), Some(kept));
    let mut frames = 0;
    while ui.reclaiming() {
        ui.prepare_frame();
        frames += 1;
    }
    assert!(frames < 200, "freed over a bounded number of frames");
    assert_eq!(ui.semantics.borrow().iter().count(), {
        let scene = ui.scene.borrow();
        ui.semantics
            .borrow()
            .iter()
            .filter(|(id, _)| scene.contains(*id))
            .count()
    });
}

#[test]
fn its_components_resources_drop_at_once_and_the_rest_over_frames() {
    let mut ui = Ui::new(800., 600.);
    let (resources, effects) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let tick = ui.signal(0_u32);
    let runs = Rc::new(Cell::new(0));
    let rows = {
        let (resources, effects, tick, runs) = (
            resources.clone(),
            effects.clone(),
            tick.clone(),
            runs.clone(),
        );
        column().children((0..400).map(move |_| {
            let (resources, effects, tick, runs) = (
                resources.clone(),
                effects.clone(),
                tick.clone(),
                runs.clone(),
            );
            component(move |cx| {
                cx.retain(Lease::new(&resources));
                let held = Lease::new(&effects);
                text_signal(move || {
                    let _held = &held;
                    runs.set(runs.get() + 1);
                    tick.get().to_string()
                })
            })
        }))
    };
    let mounted = ui.mount(rows);
    ui.prepare_frame();
    assert_eq!((resources.get(), effects.get()), (400, 400));
    let ran = runs.get();
    mounted.unmount();
    assert_eq!(resources.get(), 0, "what components keep drops as they go");
    assert_eq!(effects.get(), 400, "what effects hold goes later");
    tick.set(1);
    ui.prepare_frame();
    assert_eq!(runs.get(), ran, "and they never run again");
    while ui.reclaiming() {
        ui.prepare_frame();
    }
    assert_eq!(effects.get(), 0);
}

#[test]
fn pages_mount_and_go_again_and_again_while_earlier_ones_are_freed() {
    let mut ui = Ui::new(800., 600.);
    let (clicks, keys) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let tick = ui.signal(0_u32);
    let runs = Rc::new(Cell::new(0));
    let live = Rc::new(Cell::new(0));
    ui.prepare_frame();
    let nodes = ui.scene.borrow().len();
    let effects = ui.runtime.effect_count();
    let mut removed: Vec<NodeId> = Vec::new();
    for round in 0..40 {
        let (reads, counts, held) = (tick.clone(), runs.clone(), Lease::new(&live));
        let mounted = ui.mount(column().child(page(100, &clicks, &keys)).child(text_signal(
            move || {
                let _held = &held;
                counts.set(counts.get() + 1);
                reads.get().to_string()
            },
        )));
        ui.prepare_frame();
        // Nothing removed in an earlier round came back.
        {
            let scene = ui.scene.borrow();
            assert!(
                removed.iter().all(|id| !scene.contains(*id)),
                "round {round}"
            );
        }
        let root = mounted.find("page").unwrap();
        removed.extend(subtree(&ui, ui.scene.borrow().parent(root).unwrap()));
        let first = mounted.find("first").unwrap();
        ui.input.focus(&ui.scene, Some(first));
        mounted.unmount();
        let ran = runs.get();
        tick.set(round + 1);
        // One frame, or a few: the freeing goes on under the next mounts.
        for _ in 0..(round % 3) {
            ui.prepare_frame();
        }
        assert_eq!(runs.get(), ran, "round {round}: no removed effect ran");
        assert!(ui.input.focused().is_none_or(|id| !removed.contains(&id)));
    }
    let mut frames = 0;
    while ui.reclaiming() {
        ui.prepare_frame();
        frames += 1;
        assert!(frames < 2000, "freed within a bounded number of frames");
    }
    assert_eq!(live.get(), 0, "everything the pages held is freed");
    assert_eq!(ui.scene.borrow().len(), nodes);
    assert_eq!(ui.runtime.effect_count(), effects);
    assert_eq!(clicks.get(), 0);
}
