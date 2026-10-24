//! Percentage-sized siblings retain editor state while their window resizes.
use std::{cell::RefCell, rc::Rc, time::Duration};
use zgui::{compose::prelude::*, scene::NodeId, widgets::EditorHandle};
use zgui_desktop::{Application, WindowOptions};

type ReportNodes = (EditorHandle, NodeId, NodeId);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    Application::new()
        .window(WindowOptions {
            title: "zgui percentage sizes".into(),
            width: 640.,
            height: 420.,
            ..Default::default()
        })
        .run(move |cx| {
            let value = cx.ui.signal(
                "Resize this window. Both panels keep half the available width, and this editor wraps without losing its text or selection.".to_owned(),
            );
            let nodes = Rc::new(RefCell::new(None::<ReportNodes>));
            let report_nodes = nodes.clone();
            let scene = cx.ui.scene.clone();
            let viewport = cx.viewport.clone();
            let report = button()
                .size(120., 24.)
                .child(text("Report"))
                .on_click(move || {
                    let binding = report_nodes.borrow();
                    let Some((editor, split, right)) = binding.as_ref() else {
                        return;
                    };
                    let scene = scene.borrow();
                    println!(
                        "PERCENT viewport={:?} split={:?} left={:?} right={:?} caret={:?} focus={} model={:?}",
                        viewport.get(),
                        scene.bounds(*split),
                        scene.bounds(editor.node),
                        scene.bounds(*right),
                        scene.bounds(editor.caret),
                        editor.editor.borrow().selection().focus,
                        editor.value().get(),
                    );
                });
            let mounted = cx.render(
                column()
                    .w_full()
                    .h_full()
                    .p(20.)
                    .gap(12.)
                    .text_size(16.)
                    .child(report)
                    .child(
                        row()
                            .id("split")
                            .w_full()
                            .grow()
                            .flex_shrink(1.)
                            .child(
                                text_area("Resizable editor", value)
                                    .id("editor")
                                    .w_percent(50.)
                                    .h_full()
                                    .p(12.)
                                    .text_wrap(true)
                                    .bg(rgb(0x203050)),
                            )
                            .child(
                                column()
                                    .id("right")
                                    .w_percent(50.)
                                    .h_full()
                                    .p(12.)
                                    .bg(rgb(0x283b32))
                                    .child(text("50% width").text_size(20.))
                                    .child(text("Resize the window; the editor stays mounted.").text_wrap(true)),
                            ),
                    ),
            );
            cx.ui.input.focus(&cx.ui.scene, mounted.find("editor"));
            *nodes.borrow_mut() = Some((
                cx.ui.focused_editor().expect("mounted editor"),
                mounted.find("split").expect("split row"),
                mounted.find("right").expect("right panel"),
            ));
            if smoke {
                let window = cx.window.clone();
                cx.tasks.spawn(async move {
                    zgui::timer::sleep(Duration::from_secs(12)).await;
                    window.close();
                });
            }
        })
}
