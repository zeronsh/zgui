//! Keyboard focus reveals retained scroll children without application scrolling.
use std::{cell::Cell, rc::Rc, time::Duration};
use zgx::{
    compose::prelude::*,
    input::{EventPhase, InputEvent, Key},
    scene::Color,
};
use zgx_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    Application::new().window(WindowOptions {
        title: "zgx focus reveal".into(), width: 500., height: 300., ..Default::default()
    }).run(move |cx| {
        let offset = cx.ui.signal(0.0_f32);
        let reports = Rc::new(Cell::new(0));
        let view = cx.render(column().p(24.).gap(12.).text_size(16.).text_color(rgb(0xe5edf7))
            .child(text("Tab reveals focused children").h(32.).text_size(22.))
            .child(scroll(offset.clone()).id("scroll").size(300.,160.).p(8.).bg(rgb(0x203040))
                .children((0..8).map(|index| button().id(format!("row{index}")).size(284.,40.).rounded(0.)
                    .bg(Color(80, 80+index*12, 80, 255))
                    .focus(|s| s.bg(rgb(0x40a0e0)))
                    .child(text(format!("Action {index}")))))));
        if smoke {
            let nodes = (0..8).map(|i| view.find(&format!("row{i}")).unwrap()).collect::<Vec<_>>();
            let observed_offset = offset.clone(); let report_count = reports.clone();
            // Read-only instrumentation; native focus changes use framework reveal.
            cx.ui.on_event(view.node(), false, move |event| {
                if event.phase == EventPhase::Capture && matches!(&event.event, InputEvent::KeyDown { key: Key::Character(value), repeat: false, .. } if value == "r") {
                    report_count.set(report_count.get()+1);
                    let focused = nodes.iter().position(|node| *node == event.target);
                    println!("SNAPSHOT {} offset={:.3} focused={focused:?}", report_count.get(), observed_offset.get());
                }
            });
            cx.on_closed(move || println!("FOCUS_SCROLL offset={:.3} reports={}",offset.get(),reports.get()));
            let window=cx.window.clone();
            cx.tasks.spawn(async move { zgx::timer::sleep(Duration::from_secs(10)).await; window.close(); });
        }
    })
}
