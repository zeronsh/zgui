//! Initial and external editor values share the clipboard line-ending policy.
use std::time::Duration;
use zgx::{
    compose::prelude::*,
    scene::{NodeId, NodeKind, Scene},
};
use zgx_desktop::{Application, WindowOptions};

fn displayed(scene: &Scene, node: NodeId) -> Option<String> {
    if let NodeKind::Text { text, .. } = scene.kind(node) {
        return Some(text.to_string());
    }
    scene
        .children(node)
        .iter()
        .find_map(|child| displayed(scene, *child))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    Application::new()
        .window(WindowOptions {
            title: "zgx model text normalization".into(),
            width: 500.,
            height: 300.,
            ..Default::default()
        })
        .run(move |cx| {
            let single = cx.ui.signal("A\r\nB\rC\nD\tE".to_owned());
            let multi = cx.ui.signal("A\r\nB\rC\tD".to_owned());
            let replace_single = single.clone();
            let replace_multi = multi.clone();
            let replace = button()
                .size(460., 32.)
                .child(text("Replace external models"))
                .on_click(move || {
                    replace_single.set("X\r\nY\rZ\tQ".into());
                    replace_multi.set("X\r\nY\rZ\tQ".into());
                });
            let mounted = cx.render(
                column()
                    .w_full()
                    .h_full()
                    .p(20.)
                    .gap(12.)
                    .child(text("Canonical line endings, retained editors").h(24.))
                    .child(replace)
                    .child(
                        text_input("Single line", single)
                            .id("single")
                            .size(460., 40.)
                            .p(8.),
                    )
                    .child(
                        text_area("Multiple lines", multi)
                            .id("multi")
                            .size(460., 110.)
                            .p(8.),
                    ),
            );
            cx.ui.input.focus(&cx.ui.scene, mounted.find("multi"));
            let multi = cx.ui.focused_editor().expect("multiline editor");
            cx.ui.input.focus(&cx.ui.scene, mounted.find("single"));
            let single = cx.ui.focused_editor().expect("single-line editor");
            if smoke {
                let scene = cx.ui.scene.clone();
                let window = cx.window.clone();
                cx.tasks.spawn(async move {
                    for _ in 0..100 {
                        for (label, editor) in [("single", &single), ("multi", &multi)] {
                            let scene = scene.borrow();
                            let retained = scene.contains(editor.node);
                            let caret = scene.bounds(editor.caret);
                            println!(
                                "MODEL_TEXT {} model={:?} display={:?} retained={} caret_y={} caret_h={}",
                                label, editor.value().get(), displayed(&scene, editor.node).unwrap_or_default(),
                                retained, caret.y, caret.height
                            );
                        }
                        zgx::timer::sleep(Duration::from_millis(100)).await;
                    }
                    window.close();
                });
            }
        })
}
