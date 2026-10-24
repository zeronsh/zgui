//! Replace a live editor model during native composition without moving focus.
use std::{cell::Cell, rc::Rc, time::Duration};
use zgx::{
    compose::prelude::*,
    input::{EventPhase, InputEvent},
};
use zgx_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let read_only_test = std::env::args().any(|arg| arg == "--read-only-test");
    Application::new()
        .window(WindowOptions {
            title: "zgx external IME model".into(),
            width: 540.,
            height: 150.,
            ..Default::default()
        })
        .run(move |cx| {
            let value = cx.ui.signal(if read_only_test {
                "replacement".to_owned()
            } else {
                String::new()
            });
            let focus_changes = Rc::new(Cell::new(0));
            let changes = focus_changes.clone();
            let mounted = cx.render(
                column()
                    .w_full()
                    .h_full()
                    .p(20.)
                    .gap(12.)
                    .child(text("The first composition triggers a model replacement").h(24.))
                    .child(
                        text_input("Composition editor", value.clone())
                            .id("editor")
                            .size(500., 40.)
                            .p(8.)
                            .on_event(move |event| {
                                if event.phase == EventPhase::Target {
                                    match &event.event {
                                        InputEvent::Focus | InputEvent::Blur => {
                                            changes.set(changes.get() + 1)
                                        }
                                        InputEvent::ImePreedit { text, .. } => {
                                            println!("PREEDIT {text:?}")
                                        }
                                        InputEvent::ImeCommit(text) => println!("COMMIT {text:?}"),
                                        _ => {}
                                    }
                                }
                            }),
                    ),
            );
            cx.ui.input.focus(&cx.ui.scene, mounted.find("editor"));
            let editor = cx.ui.focused_editor().expect("mounted editor");
            let input = cx.ui.input.clone();
            let window = cx.window.clone();
            cx.tasks.spawn(async move {
                let mut ticks_until_replace = None;
                let mut replaced = false;
                let mut ticks_until_editable = None;
                for _ in 0..200 {
                    if let Some(ticks) = ticks_until_editable.as_mut() {
                        *ticks -= 1;
                        if *ticks == 0 {
                            editor.set_read_only(false);
                            println!("READ_ONLY_ENABLED editable={}", !editor.is_read_only());
                            ticks_until_editable = None;
                        }
                    }
                    let preedit = editor.editor.borrow().preedit().is_some();
                    if preedit && !replaced && ticks_until_replace.is_none() {
                        ticks_until_replace = Some(10);
                    }
                    if let Some(ticks) = ticks_until_replace.as_mut() {
                        *ticks -= 1;
                        if *ticks == 0 {
                            if read_only_test {
                                let had_preedit = editor.editor.borrow().preedit().is_some();
                                editor.set_read_only(true);
                                println!(
                                    "READ_ONLY_DISABLED had_preedit={} read_only={}",
                                    had_preedit,
                                    editor.is_read_only()
                                );
                                ticks_until_editable = Some(3);
                            }
                            if !read_only_test {
                                value.set("replacement".into());
                            }
                            replaced = true;
                            ticks_until_replace = None;
                        }
                    }
                    println!(
                        "EXTERNAL replaced={} preedit={} focus={} focus_changes={} model={:?}",
                        replaced,
                        editor.editor.borrow().preedit().is_some(),
                        input.focused() == Some(editor.node),
                        focus_changes.get(),
                        value.get()
                    );
                    zgx::timer::sleep(Duration::from_millis(100)).await;
                }
                window.close();
            });
        })
}
