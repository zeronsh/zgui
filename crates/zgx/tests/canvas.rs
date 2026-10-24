use std::{cell::Cell, rc::Rc};
use zgx::{
    compose::prelude::*,
    scene::{NodeKind, Rect},
    widgets::Ui,
};
#[test]
fn canvas_callback_tracks_content_and_size_with_owned_disposal() {
    let mut ui = Ui::new(320., 200.);
    let color = ui.signal(rgb(0xff0000));
    let width = ui.signal(100.);
    let calls = Rc::new(Cell::new(0));
    let view = ui.mount(
        canvas({
            let color = color.clone();
            let calls = calls.clone();
            move |(w, h)| {
                calls.set(calls.get() + 1);
                let mut canvas = Canvas::new();
                canvas.fill(Path::rectangle(Rect::new(0., 0., w, h)), color.get());
                canvas
            }
        })
        .id("canvas")
        .h(80.)
        .reactive_style({
            let width = width.clone();
            move || Styles::new().w(width.get())
        }),
    );
    frame(&mut ui);
    let first = calls.get();
    assert!(first > 0);
    let root = view.find("canvas").unwrap();
    let node = ui.scene.borrow().children(root)[0];
    assert!(matches!(ui.scene.borrow().kind(node), NodeKind::Canvas(_)));
    assert!(frame(&mut ui).is_idle());
    assert_eq!(calls.get(), first);
    color.set(rgb(0x00ff00));
    let report = frame(&mut ui);
    assert_eq!(report.layout_nodes, 0);
    assert_eq!(calls.get(), first + 1);
    width.set(150.);
    frame(&mut ui);
    assert_eq!(ui.scene.borrow().bounds(node).width, 150.);
    let before = calls.get();
    view.unmount();
    color.set(rgb(0x0000ff));
    frame(&mut ui);
    assert_eq!(calls.get(), before);
}

fn frame(ui: &mut Ui) -> zgx::scene::FrameReport {
    ui.prepare_frame();
    ui.scene.borrow_mut().flush()
}
