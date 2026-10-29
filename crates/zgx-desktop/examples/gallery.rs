//! Run: cargo run -p zgx-desktop --example gallery
#[path = "support/gallery_view.rs"]
mod gallery_view;
use zgx_desktop::{Application, WindowOptions};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = WindowOptions {
        title: "zgx — widget gallery".into(),
        width: 900.,
        height: 760.,
        ..Default::default()
    };
    Application::new().window(options).run(|cx| {
        let tasks = cx.tasks.clone();
        gallery_view::build(&mut cx.ui, move |text| {
            tasks.spawn(async move {
                zgx::timer::sleep(std::time::Duration::from_millis(200)).await;
                text.set("Async update complete".into());
            });
        });
        let ui = &mut cx.ui;
        if zgx::env::var_os("ZGX_SNAPSHOT").is_some() {
            let semantics = ui.semantics.clone();
            let viewport = cx.viewport.clone();
            cx.on_closed(move || {
                println!("VIEWPORT {:?}", viewport.get());
                for (_, node) in semantics.borrow().iter() {
                    println!("SEMANTIC {:?} {:?}", node.label, node.value);
                }
            });
        }
        if let Ok(seconds) = zgx::env::var("ZGX_SECONDS")
            .unwrap_or_default()
            .parse::<f64>()
            && seconds > 0.
        {
            let window = cx.window.clone();
            cx.tasks.spawn(async move {
                zgx::timer::sleep(std::time::Duration::from_secs_f64(seconds)).await;
                window.close();
            });
        }
    })
}
