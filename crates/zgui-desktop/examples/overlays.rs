//! Owned modal/popover children with native focus, dismissal and anchor tracking.
use std::{cell::Cell, rc::Rc, time::Duration};
use zgui::{
    compose::prelude::*,
    input::{EventPhase, InputEvent, Key},
};
use zgui_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    Application::new().window(WindowOptions{title:"zgui retained overlays".into(),width:600.,height:420.,..Default::default()})
    .run(move|cx|{
        let popup_open=cx.ui.signal(false);let modal_open=cx.ui.signal(false);let nested_open=cx.ui.signal(false);let offset=cx.ui.signal(0.0_f32);let reports=Rc::new(Cell::new(0));
        let anchor=button().id("anchor").size(180.,32.).p(0.).child(text("Open anchored popup"))
            .on_click({let open=popup_open.clone();move||{open.set(true);}});
        let popup=popover("Anchored details",popup_open.clone(),anchor).id("popup").size(180.,100.).p(12.).gap(8.).bg(rgb(0x36516e))
            .child(text("Anchored details").h(20.))
            .child(button().id("popup-close").h(32.).child(text("Close popup")).on_click({let open=popup_open.clone();move||{open.set(false);}}));
        let nested_anchor=button().id("nested-anchor").h(32.).child(text("Open nested popup"))
            .on_click({let open=nested_open.clone();move||{open.set(true);}});
        let nested=popover("Nested details",nested_open.clone(),nested_anchor).id("nested").size(180.,80.).p(12.).bg(rgb(0x4a3768))
            .child(button().id("nested-close").h(32.).child(text("Close nested")).on_click({let open=nested_open.clone();move||{open.set(false);}}));
        let dialog=modal("Settings",modal_open.clone()).id("modal").size(320.,220.).p(16.).gap(12.).bg(rgb(0x283d54))
            .child(text("Settings").h(24.).text_size(20.))
            .child(nested)
            .child(button().id("modal-close").h(32.).child(text("Close modal")).on_click({let open=modal_open.clone();move||{open.set(false);}}));
        let open_button=button().id("open-modal").size(180.,32.).shrink_0().p(0.).child(text("Open modal"))
            .on_click({let open=modal_open.clone();move||{open.set(true);}});
        let view=cx.render(column().p(24.).gap(12.).text_size(14.).text_color(rgb(0xe5edf7))
            .child(text("Retained component overlays").h(32.).shrink_0().text_size(22.))
            .child(open_button)
            .child(scroll(offset.clone()).size(300.,120.).shrink_0().bg(rgb(0x203040)).child(column().child(div().h(40.)).child(popup).child(div().h(200.))))
            .child(dialog));
        if smoke{
            let observed_nodes=[cx.ui.root(),view.find("popup").unwrap(),view.find("modal").unwrap(),view.find("nested").unwrap()];
            for observed_node in observed_nodes {
            let view=view.clone();
            let scene=cx.ui.scene.clone();let input=cx.ui.input.clone();let semantics=cx.ui.semantics.clone();
            let popup=popup_open.clone();let modal=modal_open.clone();let nested=nested_open.clone();let scroll=offset.clone();let count=reports.clone();
            cx.ui.on_event(observed_node,false,move|event|{
                if event.phase==EventPhase::Bubble{return;}
                if let InputEvent::KeyDown{key:Key::Character(key),repeat:false,..}=&event.event{
                    if key=="s"{scroll.set(40.);}
                    if key=="r"{
                        count.set(count.get()+1);let focused=input.focused().and_then(|node|semantics.borrow().get(node).map(|node|node.label.clone()));
                        println!("SNAPSHOT {} popup={} modal={} nested={} offset={:.0} focus={focused:?}",count.get(),popup.get(),modal.get(),nested.get(),scroll.get());
                        for id in ["anchor","popup","modal","nested"]{if let Some(node)=view.find(id){println!("BOUNDS {} {id} {:?}",count.get(),scene.borrow().bounds(node));}}
                    }
                }
            });
            }
            cx.on_closed(move||println!("OVERLAYS popup={} modal={} nested={} offset={:.0} reports={}",popup_open.get(),modal_open.get(),nested_open.get(),offset.get(),reports.get()));
            let window=cx.window.clone();cx.tasks.spawn(async move{zgui::timer::sleep(Duration::from_secs(18)).await;window.close();});
        }
    })
}
