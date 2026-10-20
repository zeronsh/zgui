//! Runnable native smoke check: cargo run -p zgui-desktop --example native_message_pump
#[cfg(any(target_os = "windows", target_os = "macos"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::{cell::Cell, rc::Rc, time::Duration};
    use zgui::compose::prelude::*;
    use zgui_desktop::Application;

    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    Application::new().run_with_pump(
        move || observed.set(observed.get() + 1),
        |cx| {
            cx.render(text("Native message pump smoke check"));
            let window = cx.window.clone();
            cx.tasks.spawn(async move {
                for _ in 0..20 {
                    zgui::timer::sleep(Duration::from_millis(10)).await;
                }
                window.close();
            });
        },
    )?;
    assert!(calls.get() > 0, "native message pump was never invoked");
    println!("native message pump: {} calls", calls.get());
    Ok(())
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn main() {
    eprintln!("This smoke check requires Windows or macOS");
}
