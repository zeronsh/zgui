//! Horizontal wheel scrolling, focus reveal and reactive viewport sizing.
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
        title: "zgx horizontal scroll".into(), width: 500., height: 280., ..Default::default()
    }).run(move |cx| {
        let offset=cx.ui.signal(0.0_f32); let wide=cx.ui.signal(false); let reports=Rc::new(Cell::new(0));
        let viewport=scroll_x(offset.clone()).id("scroll").h(80.).p(8.).bg(rgb(0x203040))
            .reactive_style({let wide=wide.clone(); move || Styles::new().w(if wide.get(){320.}else{240.})})
            .children((0..8).map(|i|button().id(format!("item{i}")).size(80.,64.).rounded(0.)
                .bg(Color(80,80+i*12,80,255)).focus(|s|s.bg(rgb(0x40a0e0)))
                .child(text(format!("Item {i}")))));
        let resize=button().id("resize").size(220.,44.).child(text("Enlarge viewport")).on_click({let wide=wide.clone();move || {wide.set(true);}});
        let view=cx.render(column().p(24.).gap(12.).text_size(14.).text_color(rgb(0xe5edf7))
            .child(text("Horizontal retained children").h(32.).text_size(22.))
            .child(viewport).child(resize));
        if smoke {
            let nodes=(0..8).map(|i|view.find(&format!("item{i}")).unwrap()).chain([view.find("resize").unwrap()]).collect::<Vec<_>>();
            let observed=offset.clone();let observed_wide=wide.clone();let count=reports.clone();
            cx.ui.on_event(view.node(),false,move |event|{
                if event.phase==EventPhase::Capture && matches!(&event.event,InputEvent::KeyDown{key:Key::Character(key),repeat:false,..}if key=="r") {
                    count.set(count.get()+1);
                    println!("SNAPSHOT {} offset={:.3} focused={:?} wide={}",count.get(),observed.get(),nodes.iter().position(|node|*node==event.target),observed_wide.get());
                }
            });
            cx.on_closed(move ||println!("HORIZONTAL_SCROLL offset={:.3} wide={} reports={}",offset.get(),wide.get(),reports.get()));
            let window=cx.window.clone();cx.tasks.spawn(async move {zgx::timer::sleep(Duration::from_secs(12)).await;window.close();});
        }
    })
}
