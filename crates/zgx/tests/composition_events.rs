use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use zgx::{
    compose::prelude::*,
    input::{EventPhase, InputEvent, Key, Modifiers, PointerButton},
    widgets::Ui,
};
fn down(key: Key) -> InputEvent {
    InputEvent::KeyDown {
        key,
        modifiers: Modifiers::default(),
        repeat: false,
    }
}
#[test]
fn owned_event_hooks_compose_capture_target_bubble_and_refinement_order() {
    let mut ui = Ui::new(400., 200.);
    let trace = Rc::new(RefCell::new(Vec::new()));
    let parent = trace.clone();
    let inner = trace.clone();
    let outer = trace.clone();
    let mounted = ui.mount(
        column()
            .on_event(move |cx| {
                if matches!(cx.event, InputEvent::KeyDown { .. }) {
                    parent.borrow_mut().push(("parent", cx.phase));
                }
            })
            .child(
                component(move |_| {
                    div().focusable(true).id("child").on_event(move |cx| {
                        if matches!(cx.event, InputEvent::KeyDown { .. }) {
                            inner.borrow_mut().push(("inner", cx.phase));
                        }
                    })
                })
                .on_event(move |cx| {
                    if matches!(cx.event, InputEvent::KeyDown { .. }) {
                        outer.borrow_mut().push(("outer", cx.phase));
                    }
                }),
            ),
    );
    ui.input.focus(&ui.scene, mounted.find("child"));
    ui.dispatch(down(Key::ArrowRight));
    assert_eq!(
        *trace.borrow(),
        vec![
            ("parent", EventPhase::Capture),
            ("inner", EventPhase::Target),
            ("outer", EventPhase::Target),
            ("parent", EventPhase::Bubble)
        ]
    );
    assert!(!ui.input.options(mounted.node()).unwrap().focusable);
}
#[test]
fn declarative_event_prevention_cancels_button_activation_and_hook_lifetime_is_owned() {
    struct DropCount(Rc<Cell<usize>>);
    impl Drop for DropCount {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let mut ui = Ui::new(200., 100.);
    let clicks = Rc::new(Cell::new(0));
    let clicked = clicks.clone();
    let drops = Rc::new(Cell::new(0));
    let resource = DropCount(drops.clone());
    let mounted = ui.mount(
        button()
            .child(text("Blocked"))
            .on_event(move |cx| {
                let _ = &resource;
                if matches!(cx.event, InputEvent::Activate) {
                    cx.prevent_default();
                }
            })
            .on_click(move || clicked.set(clicked.get() + 1)),
    );
    ui.input
        .dispatch_to(&ui.scene, mounted.node(), InputEvent::Activate);
    assert_eq!(clicks.get(), 0);
    assert_eq!(drops.get(), 0);
    mounted.unmount();
    assert_eq!(drops.get(), 1);
}
#[test]
fn editor_space_allows_native_text_without_activating_clickable_ancestor() {
    let mut ui = Ui::new(300., 100.);
    let value = ui.signal("a".to_owned());
    let clicks = Rc::new(Cell::new(0));
    let clicked = clicks.clone();
    let mounted = ui.mount(
        div()
            .on_click(move || clicked.set(clicked.get() + 1))
            .child(text_input("Name", value.clone()).id("editor")),
    );
    ui.input.focus(&ui.scene, mounted.find("editor"));
    let keydown = ui.dispatch(down(Key::Space));
    assert!(
        !keydown.default_prevented,
        "native host must deliver the associated Space text"
    );
    ui.dispatch(InputEvent::Text(" ".into()));
    assert!(value.get().contains(' '));
    let keyup = ui.dispatch(InputEvent::KeyUp {
        key: Key::Space,
        modifiers: Modifiers::default(),
    });
    assert!(keyup.default_prevented);
    assert_eq!(clicks.get(), 0);
}
#[test]
fn parent_capture_can_cancel_editor_text_before_model_mutation() {
    let mut ui = Ui::new(300., 100.);
    let value = ui.signal("stable".to_owned());
    let mounted = ui.mount(
        column()
            .on_event(|cx| {
                if cx.phase == EventPhase::Capture && matches!(cx.event, InputEvent::Text(_)) {
                    cx.prevent_default();
                }
            })
            .child(text_input("Name", value.clone()).id("editor")),
    );
    ui.input.focus(&ui.scene, mounted.find("editor"));
    let result = ui.dispatch(InputEvent::Text("x".into()));
    assert!(result.default_prevented);
    assert_eq!(value.get(), "stable");
}
#[test]
fn custom_component_can_capture_pointer_and_handle_keyboard_without_scene_writes() {
    let mut ui = Ui::new(300., 200.);
    let events = Rc::new(Cell::new(0));
    let calls = events.clone();
    let mounted = ui.mount(div().size(80., 40.).focusable(true).on_event(
        move |cx| match cx.event {
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                ..
            } => {
                cx.capture_pointer();
                cx.focus();
            }
            InputEvent::PointerMove { .. } => {
                calls.set(calls.get() + 1);
            }
            InputEvent::KeyDown {
                key: Key::ArrowRight,
                ..
            } => {
                calls.set(calls.get() + 1);
                cx.prevent_default();
            }
            _ => {}
        },
    ));
    ui.dispatch(InputEvent::PointerDown {
        x: 10.,
        y: 10.,
        button: PointerButton::Primary,
    });
    assert_eq!(ui.input.captured(), Some(mounted.node()));
    ui.dispatch(InputEvent::PointerMove { x: 200., y: 150. });
    ui.dispatch(down(Key::ArrowRight));
    assert_eq!(events.get(), 2);
    mounted.unmount();
    assert!(ui.input.captured().is_none());
    assert!(ui.input.focused().is_none());
}

#[test]
fn event_hooks_respect_inherited_disability_and_component_focus_override() {
    let mut ui = Ui::new(300., 100.);
    let disabled = ui.signal(true);
    let read = disabled.clone();
    let calls = Rc::new(Cell::new(0));
    let invoked = calls.clone();
    let mounted = ui.mount(
        column().disabled_when(move || read.get()).child(
            component(move |_| {
                button().id("button").on_event(move |cx| {
                    if matches!(cx.event, InputEvent::Activate) {
                        invoked.set(invoked.get() + 1);
                    }
                })
            })
            .focusable(false),
        ),
    );
    let button = mounted.find("button").unwrap();
    ui.input
        .dispatch_to(&ui.scene, button, InputEvent::Activate);
    assert_eq!(calls.get(), 0);
    disabled.set(false);
    assert!(!ui.input.focus(&ui.scene, Some(button)));
    ui.input
        .dispatch_to(&ui.scene, button, InputEvent::Activate);
    assert_eq!(calls.get(), 1);
}

#[test]
fn editor_target_hook_intercepts_text_and_deletion_before_builtin_editing() {
    let mut ui = Ui::new(300., 100.);
    let value = ui.signal("stable".to_owned());
    let mounted = ui.mount(text_input("Name", value.clone()).on_event(|cx| {
        if cx.phase == EventPhase::Target
            && matches!(
                cx.event,
                InputEvent::Text(_)
                    | InputEvent::KeyDown {
                        key: Key::Backspace,
                        ..
                    }
            )
        {
            cx.prevent_default();
        }
    }));
    ui.input.focus(&ui.scene, Some(mounted.node()));
    let text = ui.dispatch(InputEvent::Text("x".into()));
    assert!(text.default_prevented);
    assert_eq!(value.get(), "stable");
    let delete = ui.dispatch(down(Key::Backspace));
    assert!(delete.default_prevented);
    assert_eq!(value.get(), "stable");
    ui.dispatch(down(Key::ArrowLeft));
    ui.dispatch(InputEvent::Text("x".into()));
    assert_eq!(value.get(), "stable");
}
