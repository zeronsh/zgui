//! Keyboard traversal across one million virtual rows with bounded mounting.
use std::{cell::Cell, rc::Rc, time::Duration};
use zgx::{
    compose::prelude::*,
    input::{EventPhase, InputEvent, Key},
};
use zgx_desktop::{Application, WindowOptions};
struct Mounted(Rc<Cell<usize>>);
impl Drop for Mounted {
    fn drop(&mut self) {
        self.0.set(self.0.get() - 1);
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    Application::new().window(WindowOptions{title:"zgx virtual keyboard".into(),width:500.,height:300.,..Default::default()})
    .run(move|cx|{
        let offset=cx.ui.signal(0.0_f32);let live=Rc::new(Cell::new(0));let built=Rc::new(Cell::new(0));let reports=Rc::new(Cell::new(0));
        let rows=virtual_list(offset.clone(),32.,1,||1_000_000,|index|index,{
            let live=live.clone();let built=built.clone();move|_,index,cx|{
                live.set(live.get()+1);built.set(built.get()+1);cx.retain(Mounted(live.clone()));
                div().size(344.,32.)
                    .child(text(format!("Row {index}")).px(12.).py(6.))
            }
        }).keyboard_navigation(true).id("list").size(360.,176.).p(8.).bg(rgb(0x203040));
        let view=cx.render(column().p(24.).gap(12.).text_size(14.).text_color(rgb(0xe5edf7))
            .child(text("One million keyboard rows").h(32.).text_size(22.)).child(rows));
        if smoke{
            let semantics=cx.ui.semantics.clone();let v=offset.clone();let l=live.clone();let b=built.clone();let r=reports.clone();
            cx.ui.on_event(view.node(),false,move|event|{
                if event.phase==EventPhase::Capture&&matches!(&event.event,InputEvent::KeyDown{key:Key::Character(key),repeat:false,..}if key=="r"){
                    r.set(r.get()+1);let s=semantics.borrow();let position=s.get(event.target).and_then(|n|n.position_in_set);
                    println!("SNAPSHOT {} offset={:.0} position={position:?} live={} built={}",r.get(),v.get(),l.get(),b.get());
                }
            });
            cx.on_closed(move||println!("VIRTUAL_KEYBOARD offset={:.0} live={} built={} reports={}",offset.get(),live.get(),built.get(),reports.get()));
            let window=cx.window.clone();cx.tasks.spawn(async move{zgx::timer::sleep(Duration::from_secs(12)).await;window.close();});
        }
    })
}
