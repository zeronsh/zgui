//! Opt-in physical-display movement probe. Run on a desktop with two displays.
//! Uses native window snapshots, not placement request state. It does not change
//! system display settings or establish mixed-scale behavior on equal-scale screens.
use std::time::{Duration, Instant};
use zgx::{compose::prelude::*, timer::sleep};
use zgx_desktop::{Application, WindowBounds, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Application::new()
        .window(WindowOptions {
            title: "zgx physical display transition".into(),
            width: 480.,
            height: 320.,
            ..Default::default()
        })
        .run(|cx| {
            let info = cx.window_info.clone();
            let label = info.clone();
            cx.render(column().p(24.).gap(16.).children([
                text("Physical display transition").text_size(24.),
                text("This window moves between attached displays and returns."),
                text_signal(move || {
                    format!(
                        "Native display: {:?}\nNative bounds: {:?}",
                        label.current_display.get(),
                        label.bounds.get()
                    )
                }),
            ]));
            let window = cx.window.clone();
            cx.tasks.spawn(async move {
                sleep(Duration::from_secs(1)).await;
                window.refresh_window_info();
                sleep(Duration::from_millis(100)).await;
                let displays = info.displays.get();
                let initial = info.bounds.get();
                let first = info.current_display.get().expect("native current display");
                println!("INITIAL display={first} bounds={initial:?} displays={displays:?}");
                let Some(other) = displays.iter().find(|d| d.index != first) else {
                    println!("SKIPPED: requires two attached physical displays");
                    window.close();
                    return;
                };
                assert!(info.capabilities.get().placement, "global placement unavailable");
                let destination = WindowBounds::new(initial.size.0, initial.size.1)
                    .at(other.position.0 + 200, other.position.1 + 200);
                for (name, expected_display, expected_bounds) in [
                    ("other", other.index, destination),
                    ("returned", first, initial),
                ] {
                    window.set_bounds(expected_bounds);
                    let deadline = Instant::now() + Duration::from_secs(10);
                    let mut stable = 0;
                    loop {
                        window.refresh_window_info();
                        sleep(Duration::from_millis(100)).await;
                        let actual = info.bounds.get();
                        if info.current_display.get() == Some(expected_display)
                            && actual == expected_bounds
                        {
                            stable += 1;
                            if stable == 10 {
                                break;
                            }
                        } else {
                            stable = 0;
                        }
                        assert!(Instant::now() < deadline,
                            "display transition timed out: {name} expected={expected_display} {expected_bounds:?} actual={:?} {actual:?} error={:?}",
                            info.current_display.get(), info.last_request_error.get());
                    }
                    println!("OBSERVED {name} display={:?} bounds={:?}",
                        info.current_display.get(), info.bounds.get());
                    sleep(Duration::from_secs(2)).await;
                }
                let mixed_scale = displays.iter().any(|d| d.scale_factor != displays[0].scale_factor);
                println!("PASS: physical display movement and return; attached_mixed_scale={mixed_scale}");
                window.close();
            });
        })
}
