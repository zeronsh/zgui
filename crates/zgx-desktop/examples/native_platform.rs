//! Native application activation and retained window geometry controls.
use std::time::Duration;
use zgx::compose::prelude::*;
use zgx_desktop::{Application, ApplicationEvent, DisplaySelector, WindowBounds, WindowOptions};
// Native macOS fullscreen animates asynchronously; request state can change
// before restored geometry arrives. Require a stable matching snapshot, bounded
// by a deadline, before issuing the next geometry command.
async fn wait_for_fullscreen(
    window: &zgx_desktop::WindowHandle,
    info: &zgx_desktop::WindowInfo,
    fullscreen: bool,
    expected_size: (u32, u32),
) {
    let deadline = std::time::Instant::now() + Duration::from_secs(12);
    let mut stable = 0;
    let mut previous = None;
    loop {
        window.refresh_window_info();
        zgx::timer::sleep(Duration::from_millis(100)).await;
        let bounds = info.bounds.get();
        if info.fullscreen.get() == fullscreen && bounds.size == expected_size {
            stable = if previous == Some(bounds) {
                stable + 1
            } else {
                1
            };
            if stable >= 10 {
                return;
            }
        } else {
            stable = 0;
        }
        previous = Some(bounds);
        assert!(
            std::time::Instant::now() < deadline,
            "fullscreen transition did not settle: expected={fullscreen} size={expected_size:?}, actual={} {bounds:?}",
            info.fullscreen.get()
        );
    }
}
fn observe_drag_event() {
    #[cfg(target_os = "macos")]
    {
        use objc2::MainThreadMarker;
        use objc2_app_kit::{NSApplication, NSEvent, NSEventType};
        let mtm = MainThreadMarker::new().expect("drag callback is on AppKit main thread");
        let app = NSApplication::sharedApplication(mtm);
        if let Some(event) = app.currentEvent() {
            let kind = event.r#type();
            let button = matches!(
                kind,
                NSEventType::LeftMouseDown
                    | NSEventType::LeftMouseUp
                    | NSEventType::LeftMouseDragged
            )
            .then(|| event.buttonNumber());
            println!(
                "DRAG_NATIVE type={kind:?} button={button:?} pressed={} window={} timestamp={}",
                NSEvent::pressedMouseButtons(),
                event.windowNumber(),
                event.timestamp()
            );
        } else {
            println!(
                "DRAG_NATIVE currentEvent=None pressed={}",
                NSEvent::pressedMouseButtons()
            );
        }
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|a| a == "--smoke");
    let decorated = std::env::args().any(|a| a == "--decorated");
    Application::new()
        .application_id("org.zgx.NativePlatform")?
        .quit_on_last_window_close(false)
        .on_application_event(|event, windows| {
            println!("APPLICATION {event:?}");
            match event {
                ApplicationEvent::OpenUrls(urls)
                    if urls.iter().any(|url| url == "zgx-test:quit") =>
                {
                    windows.quit()
                }
                ApplicationEvent::OpenUrls(urls) => {
                    windows.open(
                        WindowOptions {
                            title: "zgx received native URLs".into(),
                            width: 520.,
                            height: 240.,
                            ..Default::default()
                        },
                        move |cx| {
                            cx.render(
                                column()
                                    .p(20.)
                                    .gap(10.)
                                    .child("Received from the operating system:")
                                    .children(urls.into_iter().map(text)),
                            );
                        },
                    );
                }
                ApplicationEvent::Reopen => {
                    windows.open(
                        WindowOptions {
                            title: "zgx reopened".into(),
                            width: 320.,
                            height: 240.,
                            ..Default::default()
                        },
                        |cx| {
                            cx.render(column().p(20.).child("Reopened by the operating system"));
                            println!("REOPENED");
                        },
                    );
                }
            }
        })
        .window(WindowOptions {
            title: "zgx native platform".into(),
            bounds: Some(WindowBounds::new(420, 320).at(100, 120)),
            min_size: Some((240., 180.)),
            display: Some(DisplaySelector::Primary),
            decorations: decorated,
            ..Default::default()
        })
        .run(move |cx| {
            let drag = cx.window.clone();
            let drag_info = cx.window_info.clone();
            let close = cx.window.clone();
            let fullscreen_window = cx.window.clone();
            let fullscreen_info = cx.window_info.clone();
            let observed_bounds = cx.window_info.bounds.clone();
            cx.render(
                column()
                    .gap(10.)
                    .child(
                        button()
                            .w(420.)
                            .h(36.)
                            .child("Drag this custom titlebar")
                            .on_event(move |event| {
                                if matches!(
                                    event.event,
                                    zgx::input::InputEvent::PointerDown {
                                        button: zgx::input::PointerButton::Primary,
                                        ..
                                    }
                                ) {
                                    println!("DRAG_REQUEST {:?}", drag_info.bounds.get());
                                    observe_drag_event();
                                    drag.drag_window();
                                    println!(
                                        "DRAG_QUEUED previous_error={:?}",
                                        drag_info.last_request_error.get()
                                    );
                                }
                            }),
                    )
                    .child(
                        button()
                            .h(36.)
                            .child("Close window; retain application")
                            .on_click(move || close.close()),
                    )
                    .child(
                        button()
                            .h(36.)
                            .child("Toggle fullscreen")
                            .on_click(move || {
                                fullscreen_window.set_fullscreen(!fullscreen_info.fullscreen.get())
                            }),
                    )
                    .child(text_signal(move || {
                        format!("Bounds: {:?}", observed_bounds.get())
                    })),
            );
            cx.on_closed(|| println!("ROOT_CLOSED"));
            if smoke {
                let window = cx.window.clone();
                let info = cx.window_info.clone();
                cx.tasks.spawn(async move {
                    zgx::timer::sleep(Duration::from_millis(600)).await;
                    println!(
                        "INITIAL {:?} {:?} displays={:?}",
                        info.bounds.get(),
                        info.capabilities.get(),
                        info.displays.get()
                    );
                    let initial = info.bounds.get();
                    let display_size = info
                        .displays
                        .get()
                        .into_iter()
                        .find(|display| Some(display.index) == info.current_display.get())
                        .expect("window display")
                        .size;
                    window.set_fullscreen(true);
                    wait_for_fullscreen(&window, &info, true, display_size).await;
                    println!(
                        "FULLSCREEN {} {:?}",
                        info.fullscreen.get(),
                        info.bounds.get()
                    );
                    window.set_fullscreen(false);
                    wait_for_fullscreen(&window, &info, false, initial.size).await;
                    println!("RESTORED {} {:?}", info.fullscreen.get(), info.bounds.get());
                    window.set_min_inner_size(Some((300., 220.)));
                    window.set_bounds(WindowBounds::new(100, 80).at(220, 180));
                    zgx::timer::sleep(Duration::from_millis(600)).await;
                    println!(
                        "MINIMUM {:?} error={:?}",
                        info.bounds.get(),
                        info.last_request_error.get()
                    );
                    println!("CONTROLS_READY");
                    zgx::timer::sleep(Duration::from_secs(2)).await;
                    window.refresh_window_info();
                    zgx::timer::sleep(Duration::from_millis(100)).await;
                    println!(
                        "AFTER_DRAG {:?} error={:?}",
                        info.bounds.get(),
                        info.last_request_error.get()
                    );
                    window.close();
                });
            }
        })
}
