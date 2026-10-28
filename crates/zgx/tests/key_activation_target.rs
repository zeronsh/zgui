//! Enter and Space activate the button they went to: not the one focus
//! moved on to while the first heard them (a dialog's button that closes
//! the dialog and moves focus).
use std::{cell::Cell, rc::Rc};
use zgx::{
    compose::prelude::*,
    input::{InputEvent, Key, Modifiers},
    widgets::Ui,
};

fn run(key: Key, prevent: bool) -> usize {
    let mut ui = Ui::new(300., 100.);
    let shown = ui.signal(true);
    let read = shown.clone();
    let write = shown.clone();
    let clicks = Rc::new(Cell::new(0));
    let count = clicks.clone();
    let second_downs = Rc::new(Cell::new(0));
    let downs = second_downs.clone();
    let next = Rc::new(Cell::new(None));
    let next_read = next.clone();
    let weak = ui.downgrade();
    let view = ui.mount(
        row().children([
            switch(
                move || read.get(),
                move |on, _| {
                    if on {
                        let write = write.clone();
                        let next_read = next_read.clone();
                        let weak = weak.clone();
                        button()
                            .child(text("First"))
                            .id("first")
                            .on_event(move |cx| {
                                if matches!(
                                    cx.event,
                                    InputEvent::KeyDown {
                                        key: Key::Enter | Key::Space,
                                        ..
                                    }
                                ) {
                                    if prevent {
                                        cx.prevent_default();
                                    }
                                    write.set(false);
                                    let ui = weak.upgrade().unwrap();
                                    ui.input.focus(&ui.scene, next_read.get());
                                }
                            })
                    } else {
                        div()
                    }
                },
            ),
            button()
                .child(text("Second"))
                .id("second")
                .on_click(move || count.set(count.get() + 1))
                .on_event(move |cx| {
                    if matches!(cx.event, InputEvent::KeyDown { .. }) {
                        downs.set(downs.get() + 1);
                    }
                }),
        ]),
    );
    ui.prepare_frame();
    let first = view.find("first").unwrap();
    let second = view.find("second").unwrap();
    next.set(Some(second));
    assert!(ui.input.focus(&ui.scene, Some(first)));
    ui.dispatch(InputEvent::KeyDown {
        key: key.clone(),
        modifiers: Modifiers::default(),
        repeat: false,
    });
    assert!(!shown.get());
    assert!(view.find("first").is_none());
    assert_eq!(ui.input.focused(), Some(second));
    ui.dispatch(InputEvent::KeyUp {
        key,
        modifiers: Modifiers::default(),
    });
    assert_eq!(second_downs.get(), 0);
    clicks.get()
}

#[test]
fn removing_the_enter_target_does_not_activate_its_focus_successor() {
    assert_eq!(run(Key::Enter, false), 0);
}

#[test]
fn removing_the_space_target_does_not_press_its_focus_successor() {
    assert_eq!(run(Key::Space, false), 0);
}

#[test]
fn prevented_key_control() {
    assert_eq!(run(Key::Enter, true), 0);
    assert_eq!(run(Key::Space, true), 0);
}
