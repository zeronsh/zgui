use std::{cell::Cell, rc::Rc};
use zgx::{
    compose::prelude::*,
    input::{InputEvent, Key, Modifiers, PointerButton},
    semantics::Role,
    widgets::Ui,
};
fn key(ui: &mut Ui, key: Key) {
    ui.dispatch(InputEvent::KeyDown {
        key,
        modifiers: Modifiers::default(),
        repeat: false,
    });
}
fn position(ui: &Ui) -> Option<usize> {
    ui.input.focused().and_then(|node| {
        ui.semantics
            .borrow()
            .get(node)
            .and_then(|node| node.position_in_set)
    })
}
#[test]
fn million_row_navigation_is_bounded_and_works_in_batches() {
    let mut ui = Ui::new(400., 300.);
    let offset = ui.signal(0.);
    let builds = Rc::new(Cell::new(0));
    let count = builds.clone();
    let view = ui.mount(
        virtual_list(
            offset.clone(),
            20.,
            1,
            || 1_000_000,
            |n| n,
            move |_, n, _| {
                count.set(count.get() + 1);
                text(n.to_string())
            },
        )
        .w(200.)
        .h(100.)
        .keyboard_navigation(true),
    );
    ui.prepare_frame();
    ui.input.focus(&ui.scene, Some(view.node()));
    key(&mut ui, Key::End);
    assert_eq!(position(&ui), Some(1_000_000));
    assert_eq!(offset.get(), 19_999_900.);
    let input = ui.input.clone();
    let scene = ui.scene.clone();
    ui.runtime.batch(|| {
        input.dispatch(
            &scene,
            InputEvent::KeyDown {
                key: Key::PageUp,
                modifiers: Modifiers::default(),
                repeat: false,
            },
        );
    });
    assert_eq!(position(&ui), Some(999_995));
    key(&mut ui, Key::Home);
    assert_eq!(position(&ui), Some(1));
    key(&mut ui, Key::ArrowDown);
    assert_eq!(position(&ui), Some(2));
    assert!(builds.get() < 30);
    assert!(
        ui.semantics
            .borrow()
            .iter()
            .filter(|(_, n)| n.role == Role::ListItem)
            .count()
            <= 7
    );
}
#[test]
fn retained_key_reorder_updates_focus_index_and_removal_falls_back() {
    let mut ui = Ui::new(400., 300.);
    let offset = ui.signal(0.);
    let items = ui.signal(vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
    let count = items.clone();
    let keys = items.clone();
    let view = ui.mount(
        virtual_list(
            offset.clone(),
            20.,
            0,
            move || count.get().len(),
            move |n| keys.get()[n],
            |_, n, _| text(n.to_string()),
        )
        .w(200.)
        .h(80.)
        .keyboard_navigation(true),
    );
    ui.prepare_frame();
    ui.input.focus(&ui.scene, Some(view.node()));
    key(&mut ui, Key::ArrowDown);
    key(&mut ui, Key::ArrowDown);
    let focused = ui.input.focused().unwrap();
    assert_eq!(position(&ui), Some(2));
    items.set(vec![0, 2, 1, 3, 4, 5, 6, 7, 8, 9]);
    assert_eq!(ui.input.focused(), Some(focused));
    assert_eq!(position(&ui), Some(3));
    key(&mut ui, Key::ArrowDown);
    assert_eq!(position(&ui), Some(4));
    items.set(vec![0, 2, 1, 4, 5, 6, 7, 8, 9, 3]);
    assert_eq!(ui.input.focused(), Some(view.node()));
    key(&mut ui, Key::End);
    items.set(vec![]);
    ui.prepare_frame();
    assert_eq!(ui.input.focused(), Some(view.node()));
    assert_eq!(offset.get(), 0.);
    assert_eq!(
        ui.semantics.borrow().get(view.node()).unwrap().size_of_set,
        Some(0)
    );
}
#[test]
fn interactive_descendants_keep_keys_and_pointer_activation() {
    let mut ui = Ui::new(400., 300.);
    let offset = ui.signal(0.);
    let calls = ui.signal(0);
    let click = calls.clone();
    let value = ui.signal("abc".to_string());
    let editor = value.clone();
    let view = ui.mount(
        virtual_list(
            offset.clone(),
            32.,
            0,
            || 20,
            |n| n,
            move |_, n, _| {
                if n == 0 {
                    let click = click.clone();
                    button()
                        .w(180.)
                        .h(32.)
                        .id("button")
                        .child(text("Click"))
                        .on_click(move || {
                            click.set(click.get() + 1);
                        })
                } else if n == 1 {
                    text_input("Editor", editor.clone())
                        .w(180.)
                        .h(32.)
                        .id("editor")
                } else {
                    text(n.to_string())
                }
            },
        )
        .w(200.)
        .h(128.)
        .keyboard_navigation(true),
    );
    ui.prepare_frame();
    for event in [
        InputEvent::PointerDown {
            x: 50.,
            y: 16.,
            button: PointerButton::Primary,
        },
        InputEvent::PointerUp {
            x: 50.,
            y: 16.,
            button: PointerButton::Primary,
        },
    ] {
        ui.dispatch(event);
    }
    assert_eq!(calls.get(), 1);
    assert_eq!(ui.input.focused(), view.find("button"));
    key(&mut ui, Key::End);
    assert_eq!(offset.get(), 0.);
    assert_eq!(ui.input.focused(), view.find("button"));
    ui.input.focus(&ui.scene, view.find("editor"));
    key(&mut ui, Key::Home);
    key(&mut ui, Key::ArrowDown);
    assert_eq!(offset.get(), 0.);
    assert_eq!(ui.input.focused(), view.find("editor"));
    ui.dispatch(InputEvent::Text("x".into()));
    assert!(value.get().contains('x'));
}
#[test]
fn disabled_opt_in_cleanup_and_invalid_view_validation() {
    let mut ui = Ui::new(400., 300.);
    let offset = ui.signal(0.);
    let disabled = ui.signal(true);
    let read = disabled.clone();
    let view = ui.mount(
        virtual_list(
            offset.clone(),
            20.,
            0,
            || 100,
            |n| n,
            |_, n, _| text(n.to_string()),
        )
        .w(200.)
        .h(100.)
        .keyboard_navigation(true)
        .disabled_when(move || read.get()),
    );
    ui.prepare_frame();
    assert!(!ui.input.focus(&ui.scene, Some(view.node())));
    disabled.set(false);
    ui.input.focus(&ui.scene, Some(view.node()));
    key(&mut ui, Key::End);
    let focused = ui.input.focused().unwrap();
    view.unmount();
    assert!(!ui.input.has_listeners(focused));
    offset.set(4000.);
    assert_eq!(offset.get(), 4000.);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(
            || ui.mount(text("invalid").keyboard_navigation(true))
        ))
        .is_err()
    );
}
