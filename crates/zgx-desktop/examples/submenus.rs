//! Cascading component menus with side placement and scoped keyboard navigation.
use std::{cell::Cell, rc::Rc, time::Duration};
use zgx::{
    compose::prelude::*,
    input::{EventPhase, InputEvent, Key},
};
use zgx_desktop::{Application, WindowOptions};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    Application::new().window(WindowOptions{title:"zgx cascading menus".into(),width:700.,height:420.,..Default::default()})
    .run(move|cx|{
        let open=cx.ui.signal(false);let child_open=cx.ui.signal(false);let selections=Rc::new(Cell::new(0));let reports=Rc::new(Cell::new(0));let parent_selections=Rc::new(Cell::new(0));let background_hits=Rc::new(Cell::new(0));
        let child=submenu("More",child_open.clone()).id("child-menu").size(180.,80.).p(8.).bg(rgb(0x4a3768))
            .child(menu_item("Action").h(32.).on_click({let open=open.clone();let child=child_open.clone();let selections=selections.clone();move||{
                selections.set(selections.get()+1);println!("ACTION parent={} child={} selections={}",open.get(),child.get(),selections.get());
            }}));
        let anchor=button().size(180.,32.).p(0.).child(text("Open actions")).on_click({let open=open.clone();move||{open.set(true);}});
        let menu=menu("Actions",open.clone(),anchor).id("parent-menu").size(200.,132.).p(8.).gap(4.).bg(rgb(0x36516e))
            .children([menu_item("First").h(36.).on_click({let open=open.clone();let child=child_open.clone();let count=parent_selections.clone();move||{count.set(count.get()+1);println!("PARENT_ACTION parent={} child={} count={}",open.get(),child.get(),count.get());}}),child,menu_item("Last").h(36.)]);
        let view=cx.render(column().p(24.).gap(12.).text_size(14.).text_color(rgb(0xe5edf7))
            .child(text("Cascading component menus").h(32.).text_size(22.))
            .child(row().child(div().w(236.)).child(menu))
            .child(button().size(180.,32.).translate(0.,200.).child(text("Background action")).on_click({let hits=background_hits.clone();move||{hits.set(hits.get()+1);}})));
        if smoke{
            for node in [cx.ui.root(),view.find("parent-menu").unwrap(),view.find("child-menu").unwrap()]{
                let open=open.clone();let child=child_open.clone();let count=reports.clone();let input=cx.ui.input.clone();let semantics=cx.ui.semantics.clone();let scene=cx.ui.scene.clone();let view=view.clone();
                cx.ui.on_event(node,false,move|event|{
                    if event.phase==EventPhase::Bubble{return;}
                    if matches!(&event.event,InputEvent::KeyDown{key:Key::Character(key),repeat:false,..}if key=="r"){
                        count.set(count.get()+1);let focus=input.focused().and_then(|node|semantics.borrow().get(node).map(|node|node.label.clone()));
                        println!("SNAPSHOT {} parent={} child={} focus={focus:?}",count.get(),open.get(),child.get());
                        for id in ["parent-menu","child-menu"]{if let Some(node)=view.find(id){println!("BOUNDS {} {id} {:?}",count.get(),scene.borrow().bounds(node));}}
                    }
                });
            }
            cx.on_closed(move||println!("SUBMENUS parent={} child={} selections={} reports={} parent_actions={} background_hits={}",open.get(),child_open.get(),selections.get(),reports.get(),parent_selections.get(),background_hits.get()));
            let window=cx.window.clone();cx.tasks.spawn(async move{zgx::timer::sleep(Duration::from_secs(20)).await;window.close();});
        }
    })
}
