use std::{cell::Cell, rc::Rc};
use zgx::{
    compose::prelude::*,
    input::{EventPhase, InputEvent, Key, Modifiers},
    widgets::Ui,
};

fn key(key: Key, command: bool, shift: bool) -> InputEvent {
    InputEvent::KeyDown {
        key,
        repeat: false,
        modifiers: Modifiers {
            control: command && !cfg!(target_os = "macos"),
            meta: command && cfg!(target_os = "macos"),
            shift,
            ..Default::default()
        },
    }
}

#[test]
fn read_only_editors_block_mutations_but_allow_selection_copy_and_external_model() {
    for multiline in [false, true] {
        let mut ui = Ui::new(400., 300.);
        let value = ui.signal("abcdef".to_owned());
        let view = if multiline {
            text_area("Editor", value.clone())
        } else {
            text_input("Editor", value.clone())
        };
        let mounted = ui.mount(view.read_only(true).size(240., 100.));
        ui.prepare_frame();
        let node = mounted.node();
        assert!(ui.input.focus(&ui.scene, Some(node)));
        let editor = ui.focused_editor().unwrap();
        assert!(editor.is_read_only());
        assert!(ui.semantics.borrow().get(node).unwrap().read_only);
        assert!(ui.set_accessible_text_selection(node, 1, 4));
        assert_eq!(editor.copy(), "bcd");
        assert_eq!(editor.cut(), "bcd");
        editor.paste("replacement");
        for event in [
            InputEvent::Text("replacement".into()),
            InputEvent::ImePreedit {
                text: "候補".into(),
                cursor: None,
            },
            InputEvent::ImeCommit("候補".into()),
            InputEvent::SetValue("replacement".into()),
            key(Key::Backspace, false, false),
            key(Key::Delete, false, false),
            key(Key::Enter, false, false),
            key(Key::Character("z".into()), true, false),
            key(Key::Character("y".into()), true, false),
        ] {
            assert!(ui.dispatch(event).default_prevented);
            assert_eq!(value.get(), "abcdef");
            assert_eq!(editor.editor.borrow().text(), "abcdef");
            assert!(editor.editor.borrow().preedit().is_none());
        }
        assert!(ui.set_accessible_value(node, "ignored"));
        assert_eq!(value.get(), "abcdef");
        ui.set_accessible_text_selection(node, 1, 1);
        ui.dispatch(key(Key::ArrowRight, false, true));
        assert_eq!(editor.editor.borrow().selection().focus, 2);
        assert_eq!(editor.copy(), "b");
        value.set("model updated".into());
        assert_eq!(editor.editor.borrow().text(), "model updated");
        assert_eq!(ui.input.focused(), Some(node));
        assert!(ui.semantics.borrow().get(node).unwrap().read_only);
    }
}

#[test]
fn reactive_read_only_retains_editor_history_focus_and_disposes_its_subscription() {
    let mut ui = Ui::new(400., 300.);
    let baseline = ui.runtime.effect_count();
    let value = ui.signal("abc".to_owned());
    let read_only = ui.signal(false);
    let read = read_only.clone();
    let builds = Rc::new(Cell::new(0));
    let count = builds.clone();
    let model = value.clone();
    let mounted = ui.mount(
        component(move |_| {
            count.set(count.get() + 1);
            text_area("Editor", model).size(240., 100.)
        })
        .read_only_when(move || read.get()),
    );
    ui.prepare_frame();
    let node = mounted.node();
    ui.input.focus(&ui.scene, Some(node));
    let editor = ui.focused_editor().unwrap();
    ui.dispatch(InputEvent::Text("!".into()));
    assert_eq!(value.get(), "abc!");
    ui.set_accessible_text_selection(node, 1, 3);
    let selection = editor.editor.borrow().selection();
    ui.dispatch(InputEvent::ImePreedit {
        text: "候補".into(),
        cursor: None,
    });
    let cancellation = editor.editor.borrow().composition_cancel_revision();
    read_only.set(true);
    assert!(editor.is_read_only());
    assert!(editor.editor.borrow().preedit().is_none());
    assert_ne!(
        editor.editor.borrow().composition_cancel_revision(),
        cancellation
    );
    assert_eq!(editor.editor.borrow().selection(), selection);
    assert_eq!(ui.input.focused(), Some(node));
    ui.dispatch(key(Key::Character("z".into()), true, false));
    assert_eq!(value.get(), "abc!");
    read_only.set(false);
    assert!(!editor.is_read_only());
    assert_eq!(editor.editor.borrow().selection(), selection);
    ui.dispatch(key(Key::Character("z".into()), true, false));
    assert_eq!(value.get(), "abc");
    assert_eq!(builds.get(), 1);
    assert_eq!(mounted.node(), node);
    mounted.unmount();
    assert_eq!(ui.runtime.effect_count(), baseline);
    read_only.set(true);
    value.set("after removal".into());
    assert!(!editor.is_read_only());
    assert_eq!(editor.editor.borrow().text(), "abc");
}

#[test]
fn wrapper_override_and_event_prevention_remain_compositional() {
    let mut ui = Ui::new(300., 200.);
    let value = ui.signal("stable".to_owned());
    let model = value.clone();
    let mounted = ui.mount(
        component(move |_| text_input("Editor", model).read_only(true))
            .read_only(false)
            .on_event(|cx| {
                if cx.phase == EventPhase::Target && matches!(cx.event, InputEvent::Text(_)) {
                    cx.prevent_default();
                }
            }),
    );
    ui.prepare_frame();
    ui.input.focus(&ui.scene, Some(mounted.node()));
    assert!(!ui.focused_editor().unwrap().is_read_only());
    ui.dispatch(InputEvent::Text("blocked by user handler".into()));
    assert_eq!(value.get(), "stable");
    assert!(ui.set_accessible_value(mounted.node(), "editable"));
    assert_eq!(value.get(), "editable");
    let before = ui.scene.borrow().children(ui.root()).to_vec();
    let effects = ui.runtime.effect_count();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ui.mount(column().read_only(true));
        }))
        .is_err()
    );
    assert_eq!(ui.scene.borrow().children(ui.root()), before);
    assert_eq!(ui.runtime.effect_count(), effects);
}
