//! Inherited line height reflows a retained editor without changing its model.
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};
use zgx::{compose::prelude::*, widgets::EditorHandle};
use zgx_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    Application::new()
        .window(WindowOptions {
            title: "zgx line height".into(),
            width: 600.,
            height: 340.,
            ..Default::default()
        })
        .run(move |cx| {
            let model = cx
                .ui
                .signal("First line\nSecond line\nThird line".to_owned());
            let height = cx.ui.signal(Some(24.));
            let read_height = height.clone();
            let editor = Rc::new(RefCell::new(None::<EditorHandle>));
            let report_editor = editor.clone();
            let scene = cx.ui.scene.clone();
            let stage = Cell::new(0);
            let mut controls = row().gap(8.);
            for (label, next) in [
                ("24 px", Some(24.)),
                ("42 px", Some(42.)),
                ("12 px", Some(12.)),
                ("Normal", None),
            ] {
                let height = height.clone();
                controls = controls.child(button().size(100., 40.).child(text(label)).on_click(
                    move || {
                        height.set(next);
                    },
                ));
            }
            controls = controls.child(button().size(100., 40.).child(text("Report")).on_click(
                move || {
                    let binding = report_editor.borrow();
                    let Some(editor) = binding.as_ref() else {
                        return;
                    };
                    let selection = editor.editor.borrow().selection();
                    let caret = scene.borrow().bounds(editor.caret);
                    stage.set(stage.get() + 1);
                    println!(
                        "LEADING {} height={} y={} anchor={} focus={} model={:?}",
                        stage.get(),
                        caret.height,
                        caret.y,
                        selection.anchor,
                        selection.focus,
                        editor.value().get()
                    );
                },
            ));
            let mounted = cx.render(
                column()
                    .p(20.)
                    .gap(12.)
                    .child(
                        text("Inherited line height; selection stays in place")
                            .h(32.)
                            .text_size(20.),
                    )
                    .child(
                        column()
                            .size(540., 180.)
                            .reactive_style(move || match read_height.get() {
                                Some(height) => Styles::new().line_height(height),
                                None => Styles::new().line_height_normal(),
                            })
                            .child(
                                text_area("Document", model)
                                    .id("editor")
                                    .size(540., 180.)
                                    .p(8.)
                                    .text_size(20.)
                                    .text_wrap(true),
                            ),
                    )
                    .child(controls),
            );
            cx.ui.input.focus(&cx.ui.scene, mounted.find("editor"));
            *editor.borrow_mut() = cx.ui.focused_editor();
            if smoke {
                let window = cx.window.clone();
                cx.tasks.spawn(async move {
                    zgx::timer::sleep(Duration::from_secs(10)).await;
                    window.close();
                });
            }
        })
}
