//! Typed contextual actions, function keys, timed chord replay and file-drop routing.
use zgx::{
    actions::{Action, KeyBinding, Keymap, Keystroke},
    compose::prelude::*,
    input::{EventPhase, InputEvent, Key, Modifiers},
    scene::Color,
};
use zgx_desktop::{Application, WindowOptions};
#[derive(Clone)]
struct Save;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    Application::new()
        .window(WindowOptions {
            title: "zgx actions".into(),
            width: 620.,
            height: 300.,
            ..Default::default()
        })
        .run(|window| {
            let status = window
                .ui
                .runtime
                .signal("Focus the editor, then press F5 or F1 followed by F2.".to_owned());
            let value = window
                .ui
                .runtime
                .signal("Contextual keyboard commands".to_owned());
            let binding = |keys: Vec<Key>| {
                KeyBinding::new(
                    keys.into_iter()
                        .map(|key| Keystroke::new(key, Modifiers::default())),
                    Action::new(Save),
                )
                .unwrap()
                .when("editor")
            };
            let map = Keymap::new()
                .bind(binding(vec![Key::Function(5)]))
                .bind(binding(vec![Key::Function(1), Key::Function(2)]))
                .bind(binding(vec![
                    Key::Character("x".into()),
                    Key::Character("y".into()),
                ]));
            let saved = status.clone();
            let message = status.clone();
            let dropped = status.clone();
            let editor = value.clone();
            window.render(component(move |cx| {
                if std::env::var_os("ZGX_ACTION_TRACE").is_some() {
                    let observed = value.clone();
                    let trace = cx
                        .runtime()
                        .effect(move || println!("ACTION_MODEL {:?}", observed.get()));
                    cx.retain(trace);
                }
                column()
                    .p(20.)
                    .gap(16.)
                    .bg(Color(24, 28, 36, 255))
                    .text_color(Color(235, 239, 248, 255))
                    .keymap(map)
                    .on_action(move |_: &Save, cx| {
                        saved.set(format!("Saved: {}", editor.get()));
                        if std::env::var_os("ZGX_ACTION_TRACE").is_some() {
                            println!("ACTION_SAVE {:?}", editor.get());
                        }
                        cx.prevent_default();
                        cx.stop_propagation();
                    })
                    .on_event(move |cx| {
                        if std::env::var_os("ZGX_ACTION_EVENTS").is_some()
                            && cx.phase == EventPhase::Capture
                        {
                            eprintln!("ACTION_EVENT {:?}", cx.event);
                        }
                        if cx.phase == EventPhase::Capture {
                            return;
                        }
                        if let InputEvent::FileDrop { path, .. } = &cx.event {
                            dropped.set(format!("Dropped: {}", path.display()));
                            cx.prevent_default();
                        }
                    })
                    .child(text("Typed actions and native file drops"))
                    .child(
                        text_input("Document", value)
                            .w(560.)
                            .h(40.)
                            .keymap(Keymap::new().context("editor")),
                    )
                    .child(text_signal(move || message.get()).text_wrap(true).w(560.))
                    .child(text("Drop a file anywhere inside this panel."))
            }));
        })
}
