//! Retained menu items with keyboard navigation, disability and owned dismissal.
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};
use zgx::{
    compose::prelude::*,
    input::{EventPhase, InputEvent, Key},
};
use zgx_desktop::{Application, WindowOptions};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    Application::new().window(WindowOptions{title:"zgx component menus".into(),width:500.,height:340.,..Default::default()})
    .run(move|cx|{
        let open=cx.ui.signal(false);let reports=Rc::new(Cell::new(0));let selections=Rc::new(RefCell::new(Vec::<String>::new()));
        let item=|label:&'static str|{
            let open=open.clone();let selections=selections.clone();
            menu_item(label).h(32.).px(12.).py(4.).rounded(0.)
                .focus(|s|s.bg(rgb(0x42678c)))
                .on_click(move||{println!("SELECT {label} open={}",open.get());selections.borrow_mut().push(label.into());})
        };
        let anchor=button().id("anchor").size(180.,32.).p(0.).child(text("Open menu"))
            .on_click({let open=open.clone();move||{open.set(true);}});
        let menu=menu("Actions",open.clone(),anchor).id("menu").size(220.,156.).p(8.).gap(4.).bg(rgb(0x36516e))
            .children([item("First"),item("Unavailable").disabled(true).disabled_style(|s|s.text_color(rgb(0x8994a2))),item("Middle"),item("Last")]);
        let view=cx.render(column().p(24.).gap(12.).text_size(16.).text_color(rgb(0xe5edf7))
            .child(text("Component-owned menu").h(32.).text_size(22.)).child(menu).child(button().size(180.,32.).child(text("Next control"))));
        if smoke{
            for node in [cx.ui.root(),view.find("menu").unwrap()]{
                let open=open.clone();let reports=reports.clone();let selections=selections.clone();let input=cx.ui.input.clone();let semantics=cx.ui.semantics.clone();
                cx.ui.on_event(node,false,move|event|{
                    if event.phase==EventPhase::Bubble{return;}
                    if matches!(&event.event,InputEvent::KeyDown{key:Key::Character(key),repeat:false,..}if key=="r"){
                        reports.set(reports.get()+1);let focus=input.focused().and_then(|node|semantics.borrow().get(node).map(|node|node.label.clone()));
                        println!("SNAPSHOT {} open={} focus={focus:?} selections={:?}",reports.get(),open.get(),selections.borrow());
                    }
                });
            }
            cx.on_closed(move||println!("MENUS open={} selections={:?} reports={}",open.get(),selections.borrow(),reports.get()));
            let window=cx.window.clone();cx.tasks.spawn(async move{zgx::timer::sleep(Duration::from_secs(14)).await;window.close();});
        }
    })
}
