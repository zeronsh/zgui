use std::{cell::Cell, rc::Rc};
use zgx::{
    compose::prelude::*,
    input::{InputEvent, Key, Modifiers, PointerButton},
    scene::{Color, NodeId, NodeKind},
    widgets::Ui,
};
fn fill(ui: &Ui, node: NodeId) -> Color {
    match ui.scene.borrow().kind(node) {
        NodeKind::Panel { quad, .. } => quad.fill,
        other => panic!("expected panel {other:?}"),
    }
}
fn key(ui: &mut Ui, down: bool) {
    ui.dispatch(if down {
        InputEvent::KeyDown {
            key: Key::Space,
            modifiers: Modifiers::default(),
            repeat: false,
        }
    } else {
        InputEvent::KeyUp {
            key: Key::Space,
            modifiers: Modifiers::default(),
        }
    });
}
#[test]
fn disabling_pressed_button_or_ancestor_clears_active_and_blocks_late_release() {
    for ancestor in [false, true] {
        for keyboard in [false, true] {
            let mut ui = Ui::new(300., 200.);
            let disabled = ui.signal(false);
            let read = disabled.clone();
            let count = Rc::new(Cell::new(0));
            let clicks = count.clone();
            let base = rgb(0x112233);
            let active = rgb(0xaabbcc);
            let mut control = button()
                .id("button")
                .size(100., 40.)
                .bg(base)
                .active(move |s| s.bg(active))
                .on_click(move || clicks.set(clicks.get() + 1));
            if !ancestor {
                let read = read.clone();
                control = control.disabled_when(move || read.get());
            }
            let mut parent = column().child(control);
            if ancestor {
                parent = parent.disabled_when(move || read.get());
            }
            let view = ui.mount(parent);
            ui.prepare_frame();
            let node = view.find("button").unwrap();
            ui.input.focus(&ui.scene, Some(node));
            if keyboard {
                key(&mut ui, true);
            } else {
                ui.dispatch(InputEvent::PointerDown {
                    x: 20.,
                    y: 20.,
                    button: PointerButton::Primary,
                });
            }
            ui.prepare_frame();
            assert_eq!(fill(&ui, node), active);
            disabled.set(true);
            ui.prepare_frame();
            disabled.set(false);
            ui.prepare_frame();
            assert_eq!(
                fill(&ui, node),
                base,
                "ancestor={ancestor} keyboard={keyboard}"
            );
            assert_eq!(ui.input.captured(), None);
            if keyboard {
                key(&mut ui, false);
            } else {
                ui.dispatch(InputEvent::PointerUp {
                    x: 20.,
                    y: 20.,
                    button: PointerButton::Primary,
                });
            }
            assert_eq!(count.get(), 0);
            ui.input.focus(&ui.scene, Some(node));
            key(&mut ui, true);
            key(&mut ui, false);
            ui.prepare_frame();
            assert_eq!(count.get(), 1);
            assert_eq!(fill(&ui, node), base);
        }
    }
}

#[test]
fn moving_focus_clears_held_keyboard_style_and_cannot_activate_old_target() {
    let mut ui = Ui::new(300., 200.);
    let count = Rc::new(Cell::new(0));
    let clicks = count.clone();
    let base = rgb(0x112233);
    let active = rgb(0xaabbcc);
    let view = ui.mount(
        column().children([
            button()
                .id("first")
                .size(100., 40.)
                .bg(base)
                .active(move |s| s.bg(active))
                .on_click(move || clicks.set(clicks.get() + 1)),
            button().id("second").size(100., 40.),
        ]),
    );
    ui.prepare_frame();
    let first = view.find("first").unwrap();
    let second = view.find("second").unwrap();
    ui.input.focus(&ui.scene, Some(first));
    key(&mut ui, true);
    ui.prepare_frame();
    assert_eq!(fill(&ui, first), active);
    ui.input.focus(&ui.scene, Some(second));
    ui.prepare_frame();
    assert_eq!(fill(&ui, first), base);
    key(&mut ui, false);
    assert_eq!(count.get(), 0);
}
