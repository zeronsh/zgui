//! Private: selections in a real window, for before and after captures.
use std::time::Duration;
use zgx::{compose::prelude::*, selection::TextPoint};
use zgx_desktop::{Application, WindowOptions};

const PROSE: &str = "Selected text in a real window wraps over several lines of a paragraph, \
                     so one highlight meets the next, longer or shorter, and each corner \
                     where they meet shows whether the shape is continuous.";
const LINES: &str = "fn main() {\n    let short = 1;\n\n    println!(\"a much longer line\");\n}\n";
const MIXED: &str = "English then עברית מעורבת בתוך השורה and back to English, wrapping with \
                     العربية in the same paragraph so runs split.";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Application::new()
        .window(WindowOptions {
            title: "zgx selection look".into(),
            width: 720.0,
            height: 600.0,
            ..Default::default()
        })
        .run(move |cx| {
            cx.ui
                .theme()
                .follow_system(Some((zgx::theme::DARK, zgx::theme::LIGHT)));
            let source = cx.ui.signal(LINES.to_owned());
            let para = |t: &'static str, line: f32| {
                text(t).text_size(14.0).line_height(line).w(640.0).text_wrap(true)
            };
            cx.render(
                column()
                    .p(20.0)
                    .gap(14.0)
                    .selectable(true)
                    .child(para(PROSE, 20.0))
                    .child(para(PROSE, 15.0))
                    .child(para(LINES, 20.0))
                    .child(para(MIXED, 20.0))
                    .child(text_area("code", source).size(640.0, 110.0).text_size(14.0)),
            );
            let weak = cx.ui.downgrade();
            cx.tasks.spawn(async move {
                zgx::timer::sleep(Duration::from_millis(600)).await;
                let Some(ui) = weak.upgrade() else { return };
                let mut texts = Vec::new();
                let mut code = None;
                {
                    let scene = ui.scene.borrow();
                    let mut stack = vec![scene.root()];
                    while let Some(id) = stack.pop() {
                        if ui.editor(id).is_some() {
                            code = Some(id);
                            continue;
                        }
                        if matches!(scene.kind(id), zgx::scene::NodeKind::Text { .. }) {
                            texts.push(id);
                        }
                        stack.extend(scene.children(id).iter().rev().copied());
                    }
                }
                ui.select_text(
                    TextPoint { node: texts[0], offset: 28 },
                    TextPoint { node: texts[3], offset: 96 },
                );
                ui.input.focus(&ui.scene, code);
                if let Some(editor) = ui.focused_editor() {
                    editor.set_selection(20, 60);
                }
                println!("SELECTED {} texts, editor {}", texts.len(), code.is_some());
            });
            println!("READY");
        })
}
