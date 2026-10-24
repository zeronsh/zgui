//! Native visual-page navigation through a responsive Unicode text area.
use std::time::Duration;
use zgx::{compose::prelude::*, scene::NodeKind};
use zgx_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    Application::new().window(WindowOptions {
        title: "zgx editor paging".into(), width: 560., height: 380.,
        ..Default::default()
    }).run(move |cx| {
        let original = (0..24).map(|index| format!("{index:02} Unicode 世界 e\u{301} 👩\u{200d}💻 — wrapped paragraphs keep their visual column while paging through this document.\n")).collect::<String>();
        let value = cx.ui.signal(original.clone());
        let readonly = cx.ui.signal(false);
        let read = readonly.clone();
        let viewport = cx.viewport.clone();
        let mounted = cx.render(column().p(20.).gap(12.).text_size(16.).line_height(24.)
            .child(text("PageDown / PageUp · Shift extends selection").h(24.))
            .child(text_area("Paging document", value.clone()).id("editor").p(12.).text_wrap(true)
                .read_only_when(move || read.get())
                .reactive_style(move || Styles::new().w((viewport.get().0 - 40.).max(100.))
                    .h((viewport.get().1 - 140.).max(80.))))
            .child(checkbox("Read-only (navigation stays available)",readonly.clone()).size(320.,32.)));
        cx.ui.input.focus(&cx.ui.scene, mounted.find("editor"));
        let editor = cx.ui.focused_editor().unwrap();
        if smoke {
            let scene = cx.ui.scene.clone();
            let window = cx.window.clone();
            cx.tasks.spawn(async move {
                for _ in 0..200 {
                    let selection = editor.editor.borrow().selection();
                    {
                        let scene = scene.borrow();
                        let viewport = scene.bounds(scene.parent(editor.caret).unwrap());
                        let caret = scene.bounds(editor.caret);
                        let bounds = scene.bounds(editor.node);
                        let mut pending = vec![editor.node];
                        let mut scroll = 0.;
                        while let Some(id) = pending.pop() {
                            if matches!(scene.kind(id), NodeKind::Text { .. }) {
                                scroll = viewport.y - scene.bounds(id).y;
                                break;
                            }
                            pending.extend(scene.children(id));
                        }
                        println!("PAGING anchor={} focus={} caret_y={:.2} caret_h={:.2} viewport_y={:.2} viewport_h={:.2} width={:.2} scroll={:.2} readonly={} unchanged={}",
                            selection.anchor, selection.focus, caret.y, caret.height, viewport.y, viewport.height,
                            bounds.width, scroll, readonly.get(), value.get() == original);
                    }
                    zgx::timer::sleep(Duration::from_millis(100)).await;
                }
                window.close();
            });
        }
    })
}
