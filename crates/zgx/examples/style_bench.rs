//! Per-frame cost of reactive styles, headless: boxes and labels whose
//! styles a signal animates, as an animation drives them every frame.
//! Run: cargo run -p zgx --release --example style_bench [boxes|labels]
//! (`FRAMES` sets how many frames; 600 by default.)
use std::time::Instant;
use zgx::{
    compose::{column, div, text},
    style::{Styled, Styles, rgb},
    widgets::Ui,
};

fn main() {
    let labels = std::env::args().nth(1).as_deref() == Some("labels");
    let mut ui = Ui::new(1200., 900.);
    let t = ui.signal(0_f32);
    let view = column().children((0..1000).map(|i| {
        let t = t.clone();
        let style = move || {
            let t = t.get();
            Styles::new()
                .translate(((t + i as f32) * 7.).sin() * 20., 0.)
                .opacity(0.5 + 0.5 * (t * 3. + i as f32).sin())
        };
        if labels {
            text(format!("Label {i}")).reactive_style(style)
        } else {
            div().size(20., 4.).bg(rgb(0x335577)).reactive_style(style)
        }
    }));
    ui.mount(view);
    ui.scene.borrow_mut().flush();
    let started = Instant::now();
    let frames: u32 = std::env::var("FRAMES")
        .ok()
        .and_then(|f| f.parse().ok())
        .unwrap_or(600);
    for frame in 1..=frames {
        t.set(frame as f32 / 120.);
        ui.prepare_frame();
        std::hint::black_box(ui.scene.borrow_mut().flush());
    }
    println!(
        "{}: {:.1} us per frame",
        if labels { "labels" } else { "boxes" },
        started.elapsed().as_secs_f64() * 1e6 / 600.
    );
}
