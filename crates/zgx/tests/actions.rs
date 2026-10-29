use std::{cell::RefCell, rc::Rc};
use zgx::{
    actions::{Action, KeyBinding, Keymap, Keystroke},
    compose::prelude::*,
    input::{InputEvent, Key, Modifiers},
    widgets::Ui,
};
#[derive(Debug)]
struct Save(&'static str);
fn stroke(key: Key) -> Keystroke {
    Keystroke::new(key, Modifiers::default())
}
fn binding(keys: Vec<Key>, name: &'static str) -> KeyBinding {
    KeyBinding::new(keys.into_iter().map(stroke), Action::new(Save(name))).unwrap()
}
fn down(key: Key) -> InputEvent {
    InputEvent::KeyDown {
        key,
        modifiers: Modifiers::default(),
        repeat: false,
    }
}
#[test]
fn nearest_context_binding_and_typed_bubbling_precede_editor_defaults() {
    let mut ui = Ui::new(400., 200.);
    let trace = Rc::new(RefCell::new(Vec::new()));
    let log = trace.clone();
    let value = ui.runtime.signal(String::from("hello"));
    let mounted = ui.mount(
        column()
            .keymap(
                Keymap::new()
                    .bind(binding(vec![Key::Function(5)], "global"))
                    .bind(binding(vec![Key::Backspace], "editor").when("editor")),
            )
            .on_action(move |action: &Save, cx| {
                log.borrow_mut().push(action.0);
                cx.prevent_default();
                cx.stop_propagation();
            })
            .child(
                text_input("text", value.clone())
                    .id("editor")
                    .keymap(Keymap::new().context("editor")),
            )
            .child(button().id("outside")),
    );
    ui.input.focus(&ui.scene, mounted.find("editor"));
    assert!(ui.dispatch(down(Key::Backspace)).default_prevented);
    assert_eq!(value.get(), "hello");
    ui.dispatch(down(Key::Function(5)));
    ui.input.focus(&ui.scene, mounted.find("outside"));
    ui.dispatch(down(Key::Backspace));
    assert_eq!(*trace.borrow(), vec!["editor", "global"]);
}
#[test]
fn local_bindings_shadow_global_and_unhandled_actions_bubble() {
    let mut ui = Ui::new(400., 200.);
    let trace = Rc::new(RefCell::new(Vec::new()));
    let outer = trace.clone();
    let inner = trace.clone();
    let mounted = ui.mount(
        column()
            .keymap(Keymap::new().bind(binding(vec![Key::Function(1)], "outer")))
            .on_action(move |a: &Save, cx| {
                outer.borrow_mut().push(a.0);
                cx.prevent_default();
            })
            .child(
                button()
                    .id("child")
                    .keymap(Keymap::new().bind(binding(vec![Key::Function(1)], "inner")))
                    .on_action(move |a: &Save, _| {
                        inner.borrow_mut().push(a.0);
                    }),
            ),
    );
    ui.input.focus(&ui.scene, mounted.find("child"));
    ui.dispatch(down(Key::Function(1)));
    assert_eq!(*trace.borrow(), vec!["inner", "inner"]);
    assert!(
        ui.input
            .dispatch_action(&ui.scene, Action::new(Save("native")))
            .default_prevented
    );
    assert_eq!(*trace.borrow(), vec!["inner", "inner", "native", "native"]);
}
#[test]
fn chord_consumption_mismatch_focus_cancel_and_disposal() {
    let mut ui = Ui::new(400., 200.);
    let trace = Rc::new(RefCell::new(Vec::new()));
    let log = trace.clone();
    let mounted = ui.mount(
        column()
            .keymap(
                Keymap::new()
                    .bind(binding(vec![Key::Function(1), Key::Function(2)], "chord"))
                    .bind(binding(vec![Key::Function(3)], "single")),
            )
            .on_action(move |a: &Save, cx| {
                log.borrow_mut().push(a.0);
                cx.prevent_default();
            })
            .child(button().id("a"))
            .child(button().id("b")),
    );
    ui.input.focus(&ui.scene, mounted.find("a"));
    assert!(ui.dispatch(down(Key::Function(1))).default_prevented);
    ui.dispatch(down(Key::Function(2)));
    ui.dispatch(down(Key::Function(1)));
    ui.dispatch(down(Key::Function(3)));
    ui.dispatch(down(Key::Function(1)));
    ui.input.focus(&ui.scene, mounted.find("b"));
    ui.input.focus(&ui.scene, mounted.find("a"));
    assert!(!ui.dispatch(down(Key::Function(2))).default_prevented);
    assert_eq!(*trace.borrow(), vec!["chord", "single"]);
    mounted.unmount();
    assert!(!ui.dispatch(down(Key::Function(1))).default_prevented);
}
#[test]
fn action_callback_can_remove_its_owner_and_register_another_map() {
    let mut ui = Ui::new(200., 100.);
    let owner = Rc::new(RefCell::new(None::<zgx::compose::ViewHandle>));
    let remove = owner.clone();
    let mounted = ui.mount(
        button()
            .keymap(Keymap::new().bind(binding(vec![Key::Function(1)], "remove")))
            .on_action(move |_: &Save, cx| {
                remove.borrow().as_ref().unwrap().unmount();
                cx.prevent_default();
            }),
    );
    *owner.borrow_mut() = Some(mounted.clone());
    ui.input.focus(&ui.scene, Some(mounted.node()));
    assert!(ui.dispatch(down(Key::Function(1))).default_prevented);
    assert!(!mounted.is_mounted());
}
#[test]
fn native_file_payloads_route_by_position_and_cancel_previous_hover() {
    let mut ui = Ui::new(300., 100.);
    let trace = Rc::new(RefCell::new(Vec::new()));
    let children = ["left", "right"].map(|name| {
        let trace = trace.clone();
        div().w(100.).h(100.).on_event(move |cx| {
            if cx.phase == zgx::input::EventPhase::Capture {
                return;
            }
            let event = match &cx.event {
                InputEvent::FileHover { path, .. } => format!("hover:{}", path.display()),
                InputEvent::FileDrop { path, .. } => format!("drop:{}", path.display()),
                InputEvent::FileHoverCancelled => "cancel".into(),
                _ => return,
            };
            trace.borrow_mut().push((name, event));
        })
    });
    let _mounted = ui.mount(row().children(children));
    ui.prepare_frame();
    ui.dispatch(InputEvent::FileHover {
        x: 10.,
        y: 10.,
        path: "one.txt".into(),
    });
    ui.dispatch(InputEvent::FileHover {
        x: 110.,
        y: 10.,
        path: "two.txt".into(),
    });
    ui.dispatch(InputEvent::FileDrop {
        x: 110.,
        y: 10.,
        path: "two.txt".into(),
    });
    assert_eq!(
        *trace.borrow(),
        vec![
            ("left", "hover:one.txt".into()),
            ("left", "cancel".into()),
            ("right", "hover:two.txt".into()),
            ("right", "drop:two.txt".into())
        ]
    );
}

#[test]
fn predicate_attributes_match_one_scope_and_ancestor_relations() {
    use zgx::actions::{ContextPredicate, KeyContext};
    let mut ui = Ui::new(200., 100.);
    let count = Rc::new(std::cell::Cell::new(0));
    let calls = count.clone();
    let map = Keymap::new().context("Workspace").bind(
        binding(vec![Key::Function(5)], "scoped").when_predicate(
            ContextPredicate::parse("Workspace > Editor && mode == insert && !Terminal").unwrap(),
        ),
    );
    let mounted = ui.mount(
        column()
            .keymap(map)
            .on_action(move |_: &Save, cx| {
                calls.set(calls.get() + 1);
                cx.prevent_default();
            })
            .child(
                button().id("editor").keymap(
                    Keymap::new()
                        .key_context(KeyContext::new().flag("Editor").attribute("mode", "insert")),
                ),
            )
            .child(
                button()
                    .id("terminal")
                    .keymap(Keymap::new().context("Terminal")),
            ),
    );
    ui.input.focus(&ui.scene, mounted.find("editor"));
    ui.dispatch(down(Key::Function(5)));
    ui.input.focus(&ui.scene, mounted.find("terminal"));
    ui.dispatch(down(Key::Function(5)));
    assert_eq!(count.get(), 1);
}

fn character(c: &str) -> Key {
    Key::Character(c.into())
}
fn native_text(ui: &mut Ui, c: &str) {
    let result = ui.dispatch(down(character(c)));
    if !result.default_prevented || ui.input.has_pending_keys() {
        ui.dispatch(InputEvent::Text(c.into()));
    }
    ui.dispatch(InputEvent::KeyUp {
        key: character(c),
        modifiers: Modifiers::default(),
    });
}
#[test]
fn timed_prefix_replays_native_text_once_and_clears_idle_deadline() {
    let mut ui = Ui::new(300., 100.);
    let value = ui.runtime.signal(String::new());
    let mounted = ui.mount(
        text_input("editor", value.clone())
            .keymap(Keymap::new().bind(binding(vec![character("x"), character("y")], "xy"))),
    );
    ui.input.focus(&ui.scene, Some(mounted.node()));
    assert!(ui.next_interaction_deadline().is_none());
    native_text(&mut ui, "x");
    assert_eq!(value.get(), "");
    let deadline = ui.next_interaction_deadline().unwrap();
    assert!(ui.advance_interactions(deadline).unwrap());
    assert_eq!(value.get(), "x");
    assert!(ui.next_interaction_deadline().is_none());
    assert!(!ui.advance_interactions(deadline).unwrap());
    assert_eq!(value.get(), "x");
}
#[test]
fn mismatched_prefix_replays_old_input_then_new_input_without_duplicates() {
    let mut ui = Ui::new(300., 100.);
    let value = ui.runtime.signal(String::new());
    let mounted = ui.mount(
        text_input("editor", value.clone())
            .keymap(Keymap::new().bind(binding(vec![character("x"), character("y")], "xy"))),
    );
    ui.input.focus(&ui.scene, Some(mounted.node()));
    native_text(&mut ui, "x");
    native_text(&mut ui, "z");
    assert_eq!(value.get(), "xz");
    assert!(ui.next_interaction_deadline().is_none());
}
#[test]
fn ambiguous_shortcut_waits_for_longer_binding_or_runs_shorter_on_timeout() {
    let mut ui = Ui::new(300., 100.);
    let value = ui.runtime.signal(String::new());
    let calls = Rc::new(RefCell::new(Vec::new()));
    let log = calls.clone();
    let mounted = ui.mount(
        text_input("editor", value.clone())
            .keymap(
                Keymap::new()
                    .bind(binding(vec![character("x")], "short"))
                    .bind(binding(vec![character("x"), character("y")], "long")),
            )
            .on_action(move |save: &Save, cx| {
                log.borrow_mut().push(save.0);
                cx.prevent_default();
            }),
    );
    ui.input.focus(&ui.scene, Some(mounted.node()));
    native_text(&mut ui, "x");
    assert!(calls.borrow().is_empty());
    ui.advance_interactions(ui.next_interaction_deadline().unwrap())
        .unwrap();
    assert_eq!(*calls.borrow(), vec!["short"]);
    assert_eq!(value.get(), "");
    native_text(&mut ui, "x");
    native_text(&mut ui, "y");
    assert_eq!(*calls.borrow(), vec!["short", "long"]);
    assert_eq!(value.get(), "");
    assert!(ui.next_interaction_deadline().is_none());
}
#[test]
fn unhandled_binding_falls_back_and_unhandled_chord_replays_complete_text() {
    let mut ui = Ui::new(300., 100.);
    let value = ui.runtime.signal(String::new());
    let calls = Rc::new(RefCell::new(Vec::new()));
    let log = calls.clone();
    let mounted = ui.mount(
        text_input("editor", value.clone())
            .keymap(
                Keymap::new()
                    .bind(binding(vec![Key::Function(5)], "fallback"))
                    .bind(binding(vec![Key::Function(5)], "unhandled"))
                    .bind(binding(vec![character("x"), character("y")], "unhandled")),
            )
            .on_action(move |save: &Save, cx| {
                log.borrow_mut().push(save.0);
                if save.0 == "fallback" {
                    cx.prevent_default();
                }
            }),
    );
    ui.input.focus(&ui.scene, Some(mounted.node()));
    assert!(ui.dispatch(down(Key::Function(5))).default_prevented);
    assert_eq!(*calls.borrow(), vec!["unhandled", "fallback"]);
    native_text(&mut ui, "x");
    native_text(&mut ui, "y");
    assert_eq!(value.get(), "xy");
}
#[test]
fn pending_replay_is_cancelled_by_focus_unmount_and_host_deactivation() {
    let mut ui = Ui::new(300., 100.);
    let value = ui.runtime.signal(String::new());
    let other = ui.runtime.signal(String::new());
    let mounted = ui.mount(
        column()
            .keymap(Keymap::new().bind(binding(vec![character("x"), character("y")], "xy")))
            .child(text_input("one", value.clone()).id("one"))
            .child(text_input("two", other.clone()).id("two")),
    );
    ui.input.focus(&ui.scene, mounted.find("one"));
    native_text(&mut ui, "x");
    let deadline = ui.next_interaction_deadline().unwrap();
    ui.input.focus(&ui.scene, mounted.find("two"));
    assert!(!ui.advance_interactions(deadline).unwrap());
    assert_eq!(other.get(), "");
    native_text(&mut ui, "x");
    ui.cancel_interactions();
    assert!(ui.next_interaction_deadline().is_none());
    native_text(&mut ui, "x");
    mounted.unmount();
    assert!(ui.next_interaction_deadline().is_none());
    assert_eq!(value.get(), "");
    assert_eq!(other.get(), "");
}
#[test]
fn replay_respects_custom_key_prevention_and_explicit_disabled_binding() {
    use zgx::actions::NoAction;
    let mut ui = Ui::new(300., 100.);
    let value = ui.runtime.signal(String::new());
    let mounted = ui.mount(
        text_input("editor", value.clone())
            .keymap(
                Keymap::new()
                    .bind(binding(vec![character("x"), character("y")], "xy"))
                    .bind(
                        KeyBinding::new([stroke(character("q"))], Action::new(NoAction)).unwrap(),
                    ),
            )
            .on_event(|cx| {
                if matches!(&cx.event,InputEvent::KeyDown{key:Key::Character(k),..} if k=="x") {
                    cx.prevent_default();
                }
            }),
    );
    ui.input.focus(&ui.scene, Some(mounted.node()));
    native_text(&mut ui, "x");
    ui.advance_interactions(ui.next_interaction_deadline().unwrap())
        .unwrap();
    assert_eq!(value.get(), "");
    native_text(&mut ui, "q");
    assert_eq!(value.get(), "");
}
/// What a host asks of a key a native view would otherwise keep: whether a
/// binding where focus is, or above it, takes it, as dispatch would.
#[test]
fn binds_key_answers_as_dispatch_would_without_dispatching() {
    use zgx::actions::NoAction;
    let mut ui = Ui::new(400., 200.);
    let command = Modifiers {
        meta: true,
        ..Default::default()
    };
    let at = |key: &str| Keystroke::new(character(key), command);
    let mounted = ui.mount(
        column()
            .keymap(
                Keymap::new()
                    .bind(KeyBinding::new([at("l")], Action::new(Save("app"))).unwrap())
                    .bind(KeyBinding::new([at("k"), at("p")], Action::new(Save("chord"))).unwrap())
                    .bind(
                        KeyBinding::new([at("e")], Action::new(Save("editing")))
                            .unwrap()
                            .when("editor"),
                    ),
            )
            .child(div().id("pane").focusable(true).keymap(
                Keymap::new().bind(KeyBinding::new([at("l")], Action::new(NoAction)).unwrap()),
            ))
            .child(div().id("other").focusable(true)),
    );
    ui.prepare_frame();
    let binds = |ui: &Ui, key: &str| {
        ui.input
            .binds_key(&ui.scene.borrow(), &character(key), command)
    };
    ui.input.focus(&ui.scene, mounted.find("other"));
    assert!(binds(&ui, "l"), "the app's binding above focus");
    assert!(binds(&ui, "k"), "a sequence's first stroke");
    assert!(!binds(&ui, "e"), "a binding whose context does not hold");
    assert!(!binds(&ui, "j"), "nothing bound");
    assert!(
        !ui.input
            .binds_key(&ui.scene.borrow(), &character("l"), Modifiers::default()),
        "other modifiers"
    );
    ui.input.focus(&ui.scene, mounted.find("pane"));
    assert!(!binds(&ui, "l"), "unbound nearer focus");
    assert!(binds(&ui, "k"));
    // Asking changes nothing: the chord still starts afresh.
    assert!(!ui.input.has_pending_keys());
}
#[test]
fn a_binding_by_physical_key_tells_the_keypad_from_the_digit_row() {
    let mut ui = Ui::new(400., 200.);
    let trace = Rc::new(RefCell::new(Vec::new()));
    let log = trace.clone();
    let mounted = ui.mount(
        column()
            .keymap(
                Keymap::new()
                    .bind(binding(vec![Key::Character("1".into())], "digit"))
                    .bind(KeyBinding::code(
                        "Numpad1",
                        Modifiers::default(),
                        Action::new(Save("keypad")),
                    ))
                    .bind(KeyBinding::code(
                        "NumpadEnter",
                        Modifiers {
                            shift: true,
                            ..Default::default()
                        },
                        Action::new(Save("keypad enter")),
                    )),
            )
            .on_action(move |action: &Save, cx| {
                log.borrow_mut().push(action.0);
                cx.prevent_default();
            })
            .child(button().id("focus")),
    );
    ui.input.focus(&ui.scene, mounted.find("focus"));
    // The same key ("1") from the keypad and from the digit row.
    ui.input.set_key_code(Some("Numpad1".into()));
    ui.dispatch(down(Key::Character("1".into())));
    ui.input.set_key_code(Some("Digit1".into()));
    ui.dispatch(down(Key::Character("1".into())));
    // A code binds with exactly its modifiers.
    ui.input.set_key_code(Some("NumpadEnter".into()));
    ui.dispatch(down(Key::Enter));
    ui.dispatch(InputEvent::KeyDown {
        key: Key::Enter,
        modifiers: Modifiers {
            shift: true,
            ..Default::default()
        },
        repeat: false,
    });
    assert_eq!(*trace.borrow(), vec!["keypad", "digit", "keypad enter"]);
    // A host asking whether zgx binds it sees the code binding too.
    ui.input.set_key_code(Some("Numpad1".into()));
    assert!(ui.input.binds_key(
        &ui.scene.borrow(),
        &Key::Character("x".into()),
        Modifiers::default()
    ));
}
