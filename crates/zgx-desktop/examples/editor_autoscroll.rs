//! Captured selection keeps scrolling while the pointer rests beyond an editor.
use std::{cell::RefCell, rc::Rc, time::Duration};
use zgx::{
    compose::prelude::*,
    input::{EventPhase, InputEvent, Key},
    scene::NodeKind,
    widgets::EditorHandle,
};
use zgx_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    Application::new().window(WindowOptions {
        title: "zgx editor autoscroll".into(), width: 520., height: 350.,
        ..Default::default()
    }).run(move |cx| {
        let original = (0..40).map(|i| format!("{i:02} Unicode 世界 — select a long document by holding the pointer outside.\n")).collect::<String>();
        let horizontal_original = "alpha beta 世界 gamma delta ".repeat(30);
        let value = cx.ui.signal(original.clone());
        let horizontal = cx.ui.signal(horizontal_original.clone());
        let readonly = cx.ui.signal(false);
        let read = readonly.clone();
        let handles = Rc::new(RefCell::new(Vec::<EditorHandle>::new()));
        let reports = handles.clone();
        let scene = cx.ui.scene.clone();
        let input = cx.ui.input.clone();
        let close = cx.window.clone();
        let tasks = cx.tasks.clone();
        let mounted = cx.render(column().p(20.).gap(12.).text_size(16.).line_height(24.)
            .on_event(move |event| {
                if smoke && event.phase == EventPhase::Capture && matches!(event.event, InputEvent::KeyDown {key:Key::Escape,..}) {
                    event.prevent_default();
                    close.close();
                    return;
                }
                if smoke && event.phase == EventPhase::Capture && matches!(&event.event,InputEvent::KeyDown {key:Key::Character(key),..} if key=="h") {
                    event.prevent_default();
                    close.set_visible(false);
                    let window = close.clone();
                    tasks.spawn(async move {
                        zgx::timer::sleep(Duration::from_millis(200)).await;
                        window.set_visible(true);
                        window.request_focus();
                    });
                    return;
                }
                if event.phase != EventPhase::Capture || !matches!(&event.event,InputEvent::KeyDown {key:Key::Character(key),..} if key=="r") {return;}
                event.prevent_default();
                let scene = scene.borrow();
                for (index,editor) in reports.borrow().iter().enumerate() {
                    let selection = editor.editor.borrow().selection();
                    let viewport = scene.bounds(scene.parent(editor.caret).unwrap());
                    let mut pending = vec![editor.node];
                    let text_bounds = loop {
                        let node = pending.pop().unwrap();
                        if matches!(scene.kind(node),NodeKind::Text {..}) {break scene.bounds(node);}
                        pending.extend(scene.children(node));
                    };
                    let expected = if index==0 {&original} else {&horizontal_original};
                    println!("AUTOSCROLL editor={} anchor={} focus={} x={:.2} y={:.2} captured={} readonly={} unchanged={}",
                        index,selection.anchor,selection.focus,viewport.x-text_bounds.x,viewport.y-text_bounds.y,
                        input.captured()==Some(editor.node),editor.is_read_only(),editor.value().get()==*expected);
                }
            })
            .child(text("Drag beyond an edge; hold still to keep selecting").h(24.))
            .child(text_area("Vertical document",value).id("vertical").size(480.,140.).p(12.).text_wrap(true)
                .read_only_when(move || read.get()))
            .child(text_input("Horizontal document",horizontal).id("horizontal").size(260.,40.).p(8.))
            .child(checkbox("Read-only vertical editor",readonly).size(320.,32.)));
        for id in ["vertical","horizontal"] {
            cx.ui.input.focus(&cx.ui.scene,mounted.find(id));
            handles.borrow_mut().push(cx.ui.focused_editor().unwrap());
        }
        cx.ui.input.focus(&cx.ui.scene,mounted.find("vertical"));
        if smoke {
            let window = cx.window.clone();
            cx.tasks.spawn(async move {
                // No diagnostic polling task: the native interaction deadline
                // alone must drive motion while the pointer remains stationary.
                zgx::timer::sleep(Duration::from_secs(60)).await;
                window.close();
            });
        }
    })
}
