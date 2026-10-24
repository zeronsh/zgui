//! Read-only editors retain selection/copy while external models stay writable.
use std::time::Duration;
use zgx::compose::prelude::*;
use zgx_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    Application::new()
        .window(WindowOptions {
            title: "zgx read-only editors".into(),
            width: 500.,
            height: 280.,
            ..Default::default()
        })
        .run(move |cx| {
            let readonly = cx.ui.signal(true);
            let source = cx.ui.signal("seed".to_owned());
            let destination = cx.ui.signal(String::new());
            let read = readonly.clone();
            let toggle = readonly.clone();
            let external = source.clone();
            let toggle_button = button()
                .size(460., 32.)
                .child(text("Toggle read-only"))
                .on_click(move || {
                    toggle.update(|value| *value = !*value);
                });
            let replace_button = button()
                .size(460., 32.)
                .child(text("Replace external model"))
                .on_click(move || {
                    external.set("external".into());
                });
            let mounted = cx.render(
                column()
                    .w_full()
                    .h_full()
                    .p(20.)
                    .gap(12.)
                    .child(text("Select and copy; toggle to edit").h(24.))
                    .child(toggle_button)
                    .child(
                        text_input("Read-only source", source.clone())
                            .id("source")
                            .size(460., 40.)
                            .p(8.)
                            .read_only_when(move || read.get()),
                    )
                    .child(
                        text_input("Paste destination", destination.clone())
                            .size(460., 40.)
                            .p(8.),
                    )
                    .child(replace_button),
            );
            cx.ui.input.focus(&cx.ui.scene, mounted.find("source"));
            let editor = cx.ui.focused_editor().expect("source editor");
            if smoke {
                let window = cx.window.clone();
                cx.tasks.spawn(async move {
                    for _ in 0..150 {
                        let selection = editor.editor.borrow().selection();
                        println!(
                            "READONLY enabled={} source={:?} destination={:?} anchor={} focus={}",
                            readonly.get(),
                            source.get(),
                            destination.get(),
                            selection.anchor,
                            selection.focus
                        );
                        zgx::timer::sleep(Duration::from_millis(100)).await;
                    }
                    window.close();
                });
            }
        })
}
