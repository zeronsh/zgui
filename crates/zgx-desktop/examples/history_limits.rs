//! Lowering editor history limits also bounds later undo/redo transfers.
use std::{cell::RefCell, rc::Rc, time::Duration};
use zgx::{compose::prelude::*, widgets::EditorHandle};
use zgx_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    Application::new()
        .window(WindowOptions {
            title: "zgx history limits".into(),
            width: 460.,
            height: 190.,
            ..Default::default()
        })
        .run(move |cx| {
            let value = cx.ui.signal(String::new());
            let limited = cx.ui.signal(false);
            let handle = Rc::new(RefCell::new(None::<EditorHandle>));
            let history = handle.clone();
            let reduced = limited.clone();
            let mounted = cx.render(
                column()
                    .w_full()
                    .h_full()
                    .p(20.)
                    .gap(12.)
                    .child(text("Type, undo, reduce history, redo").h(24.))
                    .child(
                        text_input("History editor", value.clone())
                            .id("editor")
                            .size(420., 40.)
                            .p(8.),
                    )
                    .child(
                        button()
                            .size(420., 40.)
                            .child(text("Reduce history to one edit"))
                            .on_click(move || {
                                if let Some(editor) = history.borrow().as_ref() {
                                    editor.editor.borrow_mut().set_history_limits(1, 1024);
                                    reduced.set(true);
                                }
                            }),
                    ),
            );
            cx.ui.input.focus(&cx.ui.scene, mounted.find("editor"));
            *handle.borrow_mut() = cx.ui.focused_editor();
            if smoke {
                let window = cx.window.clone();
                cx.tasks.spawn(async move {
                    for _ in 0..100 {
                        println!("HISTORY limited={} model={:?}", limited.get(), value.get());
                        zgx::timer::sleep(Duration::from_millis(100)).await;
                    }
                    window.close();
                });
            }
        })
}
