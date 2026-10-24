//! A native live-DPI probe with ordinary component layout and an editable field.
use std::{cell::RefCell, rc::Rc, time::Duration};
use zgui::{compose::prelude::*, text_layout::FontFamily, widgets::EditorHandle};
use zgui_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    Application::new()
        .window(WindowOptions {
            title: "zgui live DPI probe".into(),
            width: 800.,
            height: 500.,
            ..Default::default()
        })
        .run(move |cx| {
            let value = cx.ui.signal("0123456789".to_owned());
            let editor = Rc::new(RefCell::new(None::<EditorHandle>));
            let report_editor = editor.clone();
            let scene = cx.ui.scene.clone();
            let viewport = cx.viewport.clone();
            let report = button()
                .size(130., 40.)
                .child(text("Report"))
                .on_click(move || {
                    let binding = report_editor.borrow();
                    let Some(editor) = binding.as_ref() else {
                        return;
                    };
                    let scene = scene.borrow();
                    let bounds = scene.bounds(editor.node);
                    let caret = scene.bounds(editor.caret);
                    println!(
                        "DPI viewport={:?} editor={:?} caret={:?} selection={} model={:?}",
                        viewport.get(),
                        bounds,
                        caret,
                        editor.editor.borrow().selection().focus,
                        editor.value().get()
                    );
                });
            let viewport = cx.viewport.clone();
            let view = cx.render(component(move |context| {
                if smoke {
                    context.retain(
                        context
                            .runtime()
                            .effect(move || println!("VIEWPORT {:?}", viewport.get())),
                    );
                }
                column()
                    .p(20.)
                    .gap(10.)
                    .text_size(16.)
                    .font_family(FontFamily::Monospace)
                    .child(text("Live DPI transition").h(24.))
                    .child(
                        text_input("DPI editor", value)
                            .id("editor")
                            .size(400., 44.)
                            .p(8.)
                            .bg(rgb(0x203050)),
                    )
                    .child(report)
            }));
            cx.ui.input.focus(&cx.ui.scene, view.find("editor"));
            *editor.borrow_mut() = cx.ui.focused_editor();
            if smoke {
                let window = cx.window.clone();
                cx.tasks.spawn(async move {
                    zgui::timer::sleep(Duration::from_secs(15)).await;
                    window.close();
                });
            }
        })
}
