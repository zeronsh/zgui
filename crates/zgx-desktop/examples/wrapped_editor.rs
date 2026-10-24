//! Responsive, soft-wrapped plain text editing through ordinary component styles.
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};
use zgx::{compose::prelude::*, scene::NodeKind, widgets::EditorHandle};
use zgx_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    Application::new()
        .window(WindowOptions {
            title: "zgx wrapped editor".into(),
            width: 660.,
            height: 330.,
            ..Default::default()
        })
        .run(move |cx| {
            let initial = "Wrapped text follows the available width while preserving selection, Unicode input and the original model. ".repeat(8);
            let value = cx.ui.signal(initial);
            let width = cx.viewport.clone();
            let editor = Rc::new(RefCell::new(None::<EditorHandle>));
            let report_editor = editor.clone();
            let scene = cx.ui.scene.clone();
            let sequence = Rc::new(Cell::new(0));

            // Diagnostic reporting reads the same retained editor that handles
            // native input. Application layout itself uses component styles.
            let report = button()
                .size(130., 40.)
                .child(text("Report"))
                .on_click(move || {
                    let binding = report_editor.borrow();
                    let Some(editor) = binding.as_ref() else {
                        return;
                    };
                    let selection = editor.editor.borrow().selection();
                    let scene = scene.borrow();
                    let bounds = scene.bounds(editor.node);
                    let caret = scene.bounds(editor.caret);
                    let mut pending = vec![editor.node];
                    let mut text_height = 0.;
                    while let Some(node) = pending.pop() {
                        if matches!(scene.kind(node), NodeKind::Text { .. }) {
                            text_height = scene.bounds(node).height;
                            break;
                        }
                        pending.extend(scene.children(node));
                    }
                    sequence.set(sequence.get() + 1);
                    println!(
                        "WRAP {} width={:.1} text_height={:.1} anchor={} focus={} caret_y={:.1} model={:?}",
                        sequence.get(), bounds.width, text_height,
                        selection.anchor, selection.focus, caret.y, editor.value().get()
                    );
                });

            let view = cx.render(
                column()
                    .p(20.)
                    .gap(10.)
                    .text_size(16.)
                    .child(text("Soft-wrapped text editor").h(24.))
                    .child(
                        text_area("Wrapped document", value)
                            .id("editor")
                            .h(160.)
                            .p(8.)
                            .text_wrap(true)
                            .reactive_style(move || {
                                Styles::new().w((width.get().0 - 40.).max(80.))
                            }),
                    )
                    .child(report),
            );
            cx.ui.input.focus(&cx.ui.scene, view.find("editor"));
            *editor.borrow_mut() = cx.ui.focused_editor();

            if smoke {
                let window = cx.window.clone();
                cx.tasks.spawn(async move {
                    zgx::timer::sleep(Duration::from_secs(12)).await;
                    window.close();
                });
            }
        })
}
