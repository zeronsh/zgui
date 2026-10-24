//! Transparency, backdrop blur, edge fades and owned retained-group animation.
//! Run: cargo run --release -p zgx-desktop --example effects
use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};
use zgx::{
    compose::prelude::*,
    scene::{BoxShadow, Color, Transform},
};
use zgx_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    let seconds = std::env::var("ZGX_SECONDS")
        .ok()
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value > 0.)
        .or(smoke.then_some(12.));
    Application::new().window(WindowOptions {
        title: "zgx — retained composition".into(), width: 960., height: 680.,
        ..Default::default()
    }).run(move |window_context| {
        let window = window_context.window.clone();
        window_context.render(component(move |cx| {
            let opacity = cx.state(0.88);
            let blur = cx.state(10.);
            let fade = cx.state(0.);
            let animate = cx.state(false);
            let offset = cx.state(120.);
            let tasks = cx.tasks();
            let epoch = Rc::new(Cell::new(0_u64));
            cx.retain(cx.runtime().effect({
                let animate = animate.clone();
                let offset = offset.clone();
                move || {
                    let active = animate.get();
                    let generation = epoch.get().wrapping_add(1);
                    epoch.set(generation);
                    if active {
                        let epoch = epoch.clone();
                        let offset = offset.clone();
                        tasks.spawn(async move {
                            let start = Instant::now();
                            while epoch.get() == generation {
                                offset.set(120. + (start.elapsed().as_secs_f32() * 1.8).sin() * 44.);
                                zgx::timer::sleep(Duration::from_millis(16)).await;
                            }
                        });
                    }
                }
            }));
            if smoke {
                let (opacity, blur, fade, animate, offset) =
                    (opacity.clone(), blur.clone(), fade.clone(), animate.clone(), offset.clone());
                cx.tasks().spawn(async move {
                    loop {
                        println!("EFFECTS opacity={:.4} blur={:.4} fade={:.4} animate={} x={:.4}",
                            opacity.get(), blur.get(), fade.get(), animate.get(), offset.get());
                        zgx::timer::sleep(Duration::from_millis(100)).await;
                    }
                });
            }
            if let Some(seconds) = seconds {
                cx.tasks().spawn(async move {
                    zgx::timer::sleep(Duration::from_secs_f64(seconds)).await;
                    window.close();
                });
            }
            let mut root = overlay().w_full().h_full();
            for (index, color) in [Color(42,58,99,255), Color(51,86,123,255),
                Color(46,115,133,255), Color(74,143,153,255), Color(100,117,179,255)]
                .into_iter().enumerate() {
                root = root.child(div().absolute().size(128.,400.).rounded(32.).bg(color)
                    .translate(28. + index as f32 * 104., 120. + (index % 2) as f32 * 42.));
            }
            let card = overlay().absolute().size(340.,280.).isolated(true)
                .reactive_style({let (opacity,blur,fade) = (opacity.clone(),blur.clone(),fade.clone());
                    move || Styles::new().opacity(opacity.get()).blur(blur.get())
                        .edge_fade(fade.get()).translate(offset.get(),210.)})
                .child(div().absolute().size(340.,280.).bg(Color(24,30,49,180)).rounded(20.)
                    .border(1.).border_color(Color(220,232,255,100))
                    .shadow(BoxShadow {color:Color(0,0,0,130), offset:Transform{x:0.,y:12.}, blur_radius:16., spread:0.}))
                .child(column().absolute().size(340.,280.).p(24.).gap(16.)
                    .child(text("Glass and grouped opacity").size(292.,28.))
                    .child(text("Overlapping shapes blend once.").size(292.,24.))
                    .child(overlay().size(260.,100.)
                        .child(div().absolute().size(150.,80.).rounded(14.).bg(Color(81,199,204,255)))
                        .child(div().absolute().size(150.,80.).rounded(14.).bg(Color(131,114,239,255)).translate(92.,18.)))
                    .child(text("Unicode shaping · 世界 · العربية").size(296.,28.)));
            let controls = column().absolute().size(320.,450.).p(20.).gap(12.).translate(608.,130.)
                .child(text("Group opacity").size(280.,26.))
                .child(slider("Group opacity",opacity,0.0..=1.0).size(280.,32.))
                .child(text("Backdrop blur").size(280.,26.))
                .child(slider("Backdrop blur",blur,0.0..=24.0).size(280.,32.))
                .child(text("Edge fade").size(280.,26.))
                .child(slider("Edge fade",fade,0.0..=80.0).size(280.,32.))
                .child(checkbox("Animate translation",animate).size(280.,32.));
            root.child(text("Retained composition").absolute().size(860.,36.).translate(32.,30.))
                .child(text("Move and fade cached pixels. Repaint content only when it changes.")
                    .absolute().size(880.,28.).translate(32.,70.))
                .child(card).child(controls)
                .child(text("Disable blur to measure pure retained-layer movement; paused controls leave the event loop asleep.")
                    .absolute().size(890.,28.).translate(32.,612.))
        }));
    })
}
