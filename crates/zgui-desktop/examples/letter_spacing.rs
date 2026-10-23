//! Inherited tracking updates retained labels and editor geometry.
use std::{cell::RefCell, rc::Rc, time::Duration};
use zgui::{compose::prelude::*, scene::NodeId, widgets::EditorHandle};
use zgui_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    Application::new()
        .window(WindowOptions {
            title: "zgui letter spacing".into(),
            width: 600.,
            height: 300.,
            ..Default::default()
        })
        .run(move |cx| {
            let model = cx.ui.signal("ABCDEF".to_owned());
            let spacing = cx.ui.signal(0.);
            let read = spacing.clone();
            let nodes = Rc::new(RefCell::new(None::<(EditorHandle, NodeId)>));
            let report_nodes = nodes.clone();
            let scene = cx.ui.scene.clone();
            let report_spacing = spacing.clone();
            let mut controls = row().gap(8.);
            for (label, next) in [("0 px", 0.), ("3 px", 3.), ("-1 px", -1.)] {
                let spacing = spacing.clone();
                controls = controls.child(button().size(100., 40.).child(text(label)).on_click(
                    move || {
                        spacing.set(next);
                    },
                ));
            }
            controls = controls.child(button().size(100., 40.).child(text("Report")).on_click(
                move || {
                    let binding = report_nodes.borrow();
                    let Some((editor, label)) = binding.as_ref() else {
                        return;
                    };
                    let selection = editor.editor.borrow().selection();
                    let scene = scene.borrow();
                    println!(
                        "TRACKING spacing={} x={} width={} anchor={} focus={} model={:?}",
                        report_spacing.get(),
                        scene.bounds(editor.caret).x,
                        scene.bounds(*label).width,
                        selection.anchor,
                        selection.focus,
                        editor.value.get()
                    );
                },
            ));
            let mounted = cx.render(
                column()
                    .p(20.)
                    .gap(12.)
                    .child(
                        text("Inherited letter spacing; selection stays in place")
                            .h(32.)
                            .text_size(20.),
                    )
                    .child(
                        // The label as wide as its tracking makes it.
                        column()
                            .items_start()
                            .size(540., 140.)
                            .gap(8.)
                            .font_family(zgui::text_layout::FontFamily::Monospace)
                            .text_size(20.)
                            .reactive_style(move || Styles::new().letter_spacing(read.get()))
                            .child(text("ABCDEF").h(32.).id("label"))
                            .child(
                                text_area("Document", model)
                                    .id("editor")
                                    .size(540., 100.)
                                    .p(8.),
                            ),
                    )
                    .child(controls),
            );
            cx.ui.input.focus(&cx.ui.scene, mounted.find("editor"));
            *nodes.borrow_mut() = Some((
                cx.ui.focused_editor().unwrap(),
                mounted.find("label").unwrap(),
            ));
            if smoke {
                let window = cx.window.clone();
                cx.tasks.spawn(async move {
                    zgui::timer::sleep(Duration::from_secs(10)).await;
                    window.close();
                });
            }
        })
}
