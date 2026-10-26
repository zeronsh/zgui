//! Native multi-click, Shift-click and unit-drag selection in a component editor.
use std::time::Duration;
use zgx::{compose::prelude::*, scene::NodeKind, text_layout::TextPosition};
use zgx_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    Application::new().window(WindowOptions {
        title: "zgx editor selection".into(), width: 500., height: 310.,
        ..Default::default()
    }).run(move |cx| {
        let original = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu\nUnicode 世界 e\u{301} 👩\u{200d}💻 — selection preserves grapheme boundaries and the original model.".to_owned();
        let value = cx.ui.signal(original.clone());
        let readonly = cx.ui.signal(false);
        let read = readonly.clone();
        let mounted = cx.render(column().p(20.).gap(12.).text_size(16.).line_height(24.)
            .child(text("Shift-click · double-click words · triple-click paragraphs").h(24.))
            .child(text_area("Selection document",value.clone()).id("editor").size(460.,180.)
                .p(12.).text_wrap(true).read_only_when(move || read.get()))
            .child(checkbox("Read-only",readonly.clone()).size(320.,32.)));
        cx.ui.input.focus(&cx.ui.scene,mounted.find("editor"));
        let editor = cx.ui.focused_editor().unwrap();
        if smoke {
            let scene = cx.ui.scene.clone();
            let window = cx.window.clone();
            cx.tasks.spawn(async move {
                for _ in 0..400 {
                    {
                        let selection = editor.editor.borrow().selection();
                        let scene = scene.borrow();
                        let viewport = scene.bounds(scene.parent(editor.caret).unwrap());
                        let mut pending = vec![editor.node];
                        let text_node = loop {
                            let node = pending.pop().unwrap();
                            if matches!(scene.kind(node),NodeKind::Text { .. }) { break node; }
                            pending.extend(scene.children(node));
                        };
                        let NodeKind::Text {text,font_size,..} = scene.kind(text_node) else {unreachable!()};
                        let bounds = scene.bounds(text_node);
                        let layout = scene.shape_text_with_font(text,*font_size,Some(viewport.width),scene.font(text_node));
                        let point = |byte_offset| {
                            let caret = layout.caret_position(TextPosition {byte_offset,..TextPosition::default()});
                            (bounds.x+caret.x,bounds.y+caret.y+caret.height*0.5)
                        };
                        let a = point(2);
                        let b = point(8);
                        let g = point(14);
                        let second = layout.hit_position(20.,36.);
                        let start = layout.visual_line_edge(second,false).byte_offset;
                        let end = layout.visual_line_edge(second,true).byte_offset;
                        // Its paragraph, break included: what a triple click selects.
                        let at = second.byte_offset;
                        let paragraph_start = text[..at].rfind('\n').map_or(0, |i| i + 1);
                        let paragraph_end = text[at..].find('\n').map_or(text.len(), |i| at + i + 1);
                        println!("SELECTION anchor={} focus={} readonly={} unchanged={} ax={:.1} bx={:.1} gx={:.1} y={:.1} line_x={:.1} line_y={:.1} line_start={} line_end={} paragraph_start={} paragraph_end={}",
                            selection.anchor,selection.focus,readonly.get(),value.get()==original,
                            a.0,b.0,g.0,a.1,bounds.x+20.,bounds.y+36.,start,end,paragraph_start,paragraph_end);
                    }
                    zgx::timer::sleep(Duration::from_millis(100)).await;
                }
                window.close();
            });
        }
    })
}
