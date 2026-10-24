//! Overlay scrollbars with pointer capture, keyboard control and automatic extent.
use std::{cell::Cell, rc::Rc, time::Duration};
use zgx::{
    compose::prelude::*,
    input::{EventPhase, InputEvent, Key},
};
use zgx_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    Application::new().window(WindowOptions{title:"zgx overlay scrollbars".into(),width:550.,height:340.,..Default::default()})
    .run(move|cx|{
        let vertical=cx.ui.signal(0.0_f32);let horizontal=cx.ui.signal(0.0_f32);let count=cx.ui.signal(8_usize);let reports=Rc::new(Cell::new(0));
        let vertical_view=scroll(vertical.clone()).scrollbar(true).size(200.,160.).p(8.).bg(rgb(0x203040))
            .child(keyed({let count=count.clone();move||(0..count.get()).collect()},|i,_|div().size(184.,40.).bg(rgb(0x445566)).child(text(format!("Row {i}")).p(8.))));
        let horizontal_view=scroll_x(horizontal.clone()).scrollbar(true).size(240.,80.).p(8.).bg(rgb(0x203040))
            .child(switch({let count=count.clone();move||count.get()},|count,_|row().children((0..count).map(|i|div().size(80.,64.).bg(rgb(0x445566)).child(text(format!("Item {i}")).p(8.))))));
        let shrink=button().size(220.,44.).child(text("Shrink content")).on_click({let count=count.clone();move||{count.set(2);}});
        let view=cx.render(column().p(24.).gap(12.).text_size(14.).text_color(rgb(0xe5edf7))
            .child(text("Overlay scrollbars").h(32.).text_size(22.))
            .child(row().gap(16.).child(vertical_view).child(horizontal_view)).child(shrink));
        if smoke {
            let v=vertical.clone();let h=horizontal.clone();let n=count.clone();let r=reports.clone();
            cx.ui.on_event(view.node(),false,move|event|{
                if event.phase==EventPhase::Capture&&matches!(&event.event,InputEvent::KeyDown{key:Key::Character(key),repeat:false,..}if key=="r"){
                    r.set(r.get()+1);println!("SNAPSHOT {} vertical={:.3} horizontal={:.3} count={}",r.get(),v.get(),h.get(),n.get());
                }
            });
            cx.on_closed(move||println!("SCROLLBARS vertical={:.3} horizontal={:.3} count={} reports={}",vertical.get(),horizontal.get(),count.get(),reports.get()));
            let window=cx.window.clone();cx.tasks.spawn(async move{zgx::timer::sleep(Duration::from_secs(16)).await;window.close();});
        }
    })
}
