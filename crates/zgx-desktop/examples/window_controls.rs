//! Native window controls. `--smoke-test` exercises the real window manager.
use std::time::Duration;
use zgx::{compose::prelude::*, timer::sleep};
use zgx_desktop::{Application, WindowOptions};
// Observe the fixture's own AppKit window, independently of zgx's request
// state. This runs only at explicit validation checkpoints on the UI thread.
fn observe_native_state(label: &str) {
    #[cfg(target_os = "macos")]
    {
        use objc2::MainThreadMarker;
        use objc2_app_kit::NSApplication;
        let mtm = MainThreadMarker::new().expect("fixture observation runs on the UI thread");
        let app = NSApplication::sharedApplication(mtm);
        let mut found = false;
        for window in app.windows() {
            let title = window.title().to_string();
            if matches!(
                title.as_str(),
                "zgx window controls" | "zgx controls resized"
            ) {
                found = true;
                println!(
                    "NATIVE {label} title={title:?} visible={} miniaturized={} key={}",
                    window.isVisible(),
                    window.isMiniaturized(),
                    window.isKeyWindow()
                );
            }
        }
        assert!(found, "fixture's native window missing at {label}");
    }
    #[cfg(not(target_os = "macos"))]
    let _ = label;
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    Application::new().window(WindowOptions {
        title: "zgx window controls".into(), width: 640., height: 480., ..Default::default()
    }).run(move |cx| {
        let window = cx.window.clone();
        let resize = window.clone();
        let maximize = window.clone();
        let restore = window.clone();
        let minimize = window.clone();
        let minimize_tasks = cx.tasks.clone();
        let hide = window.clone();
        let hide_tasks = cx.tasks.clone();
        cx.render(column().gap(12.).p(24.).children([
            text("Native window controls"),
            button().child(text("Resize to 800 × 600")).on_click(move || resize.set_inner_size(800., 600.)),
            button().child(text("Maximize")).on_click(move || maximize.set_maximized(true)),
            button().child(text("Restore")).on_click(move || restore.set_maximized(false)),
            button().child(text("Minimize; restore in 5 seconds")).on_click(move || {
                let window = minimize.clone();
                window.set_minimized(true);
                println!("CONTROLS minimize scheduled restore in 5 seconds");
                minimize_tasks.spawn(async move {
                    sleep(Duration::from_secs(1)).await;
                    observe_native_state("manual-minimized");
                    sleep(Duration::from_secs(4)).await;
                    window.set_minimized(false);
                    window.request_focus();
                    println!("CONTROLS minimize restore and focus requested");
                    sleep(Duration::from_secs(1)).await;
                    observe_native_state("manual-restored");
                });
            }),
            button().child(text("Hide; show in 5 seconds")).on_click(move || {
                let window = hide.clone();
                window.set_visible(false);
                println!("CONTROLS hide scheduled show in 5 seconds");
                hide_tasks.spawn(async move {
                    sleep(Duration::from_secs(1)).await;
                    observe_native_state("manual-hidden");
                    sleep(Duration::from_secs(4)).await;
                    window.set_visible(true);
                    window.request_focus();
                    println!("CONTROLS show and focus requested");
                    sleep(Duration::from_secs(1)).await;
                    observe_native_state("manual-shown");
                });
            }),
        ]));
        if smoke {
            let viewport = cx.viewport.clone();
            cx.tasks.spawn(async move {
                sleep(Duration::from_secs(1)).await;
                window.set_title("zgx controls resized");
                window.set_inner_size(800., 600.);
                sleep(Duration::from_secs(1)).await;
                assert_eq!(viewport.get(), (800., 600.), "native resize was not acknowledged");
                println!("CONTROLS resized 800x600");
                window.set_minimized(true);
                println!("CONTROLS minimize requested");
                sleep(Duration::from_secs(1)).await;
                observe_native_state("smoke-minimized");
                window.set_minimized(false);
                window.request_focus();
                println!("CONTROLS restore requested");
                sleep(Duration::from_secs(1)).await;
                observe_native_state("smoke-restored");
                window.set_maximized(true);
                println!("CONTROLS maximize requested");
                sleep(Duration::from_secs(1)).await;
                assert!(viewport.get().0 >= 800. && viewport.get().1 >= 600.);
                window.set_maximized(false);
                sleep(Duration::from_secs(1)).await;
                window.set_visible(false);
                println!("CONTROLS hide requested");
                sleep(Duration::from_secs(1)).await;
                observe_native_state("smoke-hidden");
                window.set_visible(true);
                window.request_focus();
                println!("CONTROLS show requested");
                sleep(Duration::from_secs(1)).await;
                observe_native_state("smoke-shown");
                window.set_inner_size(640., 480.);
                sleep(Duration::from_secs(1)).await;
                assert_eq!(viewport.get(), (640., 480.));
                println!("window controls smoke passed (native viewport resize; state requests issued)");
                window.close();
                // Closed handles must tolerate late work from a detached task.
                window.set_title("closed");
                window.set_visible(true);
                window.set_inner_size(1., 1.);
            });
        }
    })
}
