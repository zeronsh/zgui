//! Secondary-button release must not interrupt an ongoing primary drag.
use std::time::Duration;
use zgx::compose::prelude::*;
use zgx_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    Application::new()
        .window(WindowOptions {
            title: "zgx pointer chords".into(),
            width: 540.,
            height: 190.,
            ..Default::default()
        })
        .run(move |cx| {
            let value = cx
                .ui
                .signal("abcdefghijklmnopqrstuvwxyz0123456789".to_owned());
            let amount = cx.ui.signal(10.);
            let mounted = cx.render(
                column()
                    .w_full()
                    .h_full()
                    .p(20.)
                    .gap(12.)
                    .text_size(16.)
                    .child(text("Drag while releasing the secondary button").h(24.))
                    .child(
                        text_input("Drag selection", value)
                            .id("editor")
                            .size(500., 40.)
                            .p(8.),
                    )
                    .child(slider("Drag amount", amount.clone(), 0. ..=100.).size(500., 40.)),
            );
            cx.ui.input.focus(&cx.ui.scene, mounted.find("editor"));
            let editor = cx.ui.focused_editor().expect("mounted editor");
            if smoke {
                let window = cx.window.clone();
                cx.tasks.spawn(async move {
                    for _ in 0..100 {
                        let selection = editor.editor.borrow().selection();
                        println!(
                            "CHORD anchor={} focus={} amount={}",
                            selection.anchor,
                            selection.focus,
                            amount.get()
                        );
                        zgx::timer::sleep(Duration::from_millis(100)).await;
                    }
                    window.close();
                });
            }
        })
}
