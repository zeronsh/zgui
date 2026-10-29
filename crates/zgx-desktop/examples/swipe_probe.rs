//! A trackpad's swipe between pages (macOS), by two-finger scroll gestures
//! sent to this app's own window (each a CGEvent scroll with its phases, as
//! a trackpad's arrive; the user's trackpad and other apps hear nothing).
//! The window holds a page that goes back and forward (`View::on_navigate`):
//! a row scrolling across at its top, a plain box under it. In turn:
//!
//! - `plain-back`: fingers to the right over the plain box: back;
//! - `row-toward-end`: to the left over the row at its start: it scrolls;
//! - `row-mid-back`: to the right over the row in the middle of its content:
//!   it scrolls back, and nothing navigates;
//! - `row-start-back`: to the right over the row at its start: back;
//! - `plain-forward`: to the left over the plain box: forward;
//! - `three-back`, `three-forward`: a three-finger swipe (`swipeWithEvent:`,
//!   with the system set to swipe between pages so) to the right, then the
//!   left, over the plain box: back, then forward.
//!
//! The two-finger swipes navigate only with the system's "Swipe between
//! pages" set to scroll with two fingers (never changed here). The scrolls
//! go to the window itself (`-[NSWindow sendEvent:]`): a scroll made from a
//! CGEvent has no window, so the application would send it nowhere.
//!
//! It prints `SWIPES_FROM_SCROLLS <bool>`, `STEP <name>` as each gesture
//! starts, `NAVIGATE <back|forward>` for each step the page takes, `ROW
//! <offset>` after each gesture and `SWIPE_DONE` at the end.
//! `scripts/macos_swipe_probe.py` runs it and checks the lines.
//!
//! `--manual` sends nothing: the window shows how many times it went back
//! and forward, for a person's trackpad.
use std::time::Duration;
use zgx::{compose::prelude::*, input::Navigation};
use zgx_desktop::{Application, WindowOptions};

/// The row's place in the window (points): 400 by 120 at the top left.
const ROW: (f32, f32) = (200., 60.);
/// The plain box's, under it.
const PLAIN: (f32, f32) = (300., 300.);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manual = std::env::args().any(|arg| arg == "--manual");
    Application::new()
        .window(WindowOptions {
            title: "Swipe".into(),
            width: 600.,
            height: 400.,
            ..Default::default()
        })
        .run(move |cx| {
            let offset = cx.ui.signal(0_f32);
            let went = cx.ui.signal((0, 0));
            let tiles = (0..12).fold(row(), |row, i| {
                row.child(div().w(100.).h(120.).bg(if i % 2 == 0 {
                    rgba(0x4060a0ff)
                } else {
                    rgba(0x6080c0ff)
                }))
            });
            let counts = went.clone();
            cx.render(
                column()
                    .w_full()
                    .h_full()
                    .on_navigate(move |navigation| {
                        let (back, forward) = went.get();
                        went.set(match navigation {
                            Navigation::Back => (back + 1, forward),
                            Navigation::Forward => (back, forward + 1),
                        });
                        println!("NAVIGATE {}", way(navigation));
                    })
                    .child(scroll_x(offset.clone()).w(400.).h(120.).child(tiles))
                    .child(
                        div()
                            .w_full()
                            .grow()
                            .p(16.)
                            .bg(rgba(0x80808040))
                            .child(text_signal(move || {
                                let (back, forward) = counts.get();
                                format!("back {back}, forward {forward}")
                            })),
                    ),
            );
            if manual {
                return;
            }
            let window = cx.window.clone();
            cx.tasks.spawn(async move {
                let pause = |ms| zgx::timer::sleep(Duration::from_millis(ms));
                pause(2500).await;
                println!("SWIPES_FROM_SCROLLS {}", trackpad::swipes_from_scrolls());
                let steps: [(&str, (f32, f32), f64); 5] = [
                    ("plain-back", PLAIN, 14.),
                    ("row-toward-end", ROW, -14.),
                    ("row-mid-back", ROW, 6.),
                    ("row-start-back", ROW, 14.),
                    ("plain-forward", PLAIN, -14.),
                ];
                for (name, at, delta) in steps {
                    if name == "row-start-back" {
                        offset.set(0.);
                        pause(300).await;
                    }
                    println!("STEP {name}");
                    if !trackpad::gesture(at, delta).await {
                        window.close();
                        return;
                    }
                    pause(900).await;
                    println!("ROW {}", offset.get());
                }
                for (name, delta) in [("three-back", 1.), ("three-forward", -1.)] {
                    println!("STEP {name}");
                    trackpad::three_finger_swipe(PLAIN, delta);
                    pause(500).await;
                }
                println!("SWIPE_DONE");
                window.close();
            });
        })
}

fn way(navigation: Navigation) -> &'static str {
    match navigation {
        Navigation::Back => "back",
        Navigation::Forward => "forward",
    }
}

#[cfg(target_os = "macos")]
mod trackpad {
    use objc2::{MainThreadMarker, rc::Retained};
    use objc2_app_kit::{NSApplication, NSEvent, NSScreen, NSWindow};
    use objc2_core_graphics::{CGEvent, CGEventField, CGScrollEventUnit};
    use std::time::Duration;

    /// The trackpad's scroll phases (`kCGScrollPhase…`).
    const MAY_BEGIN: i64 = 128;
    const BEGAN: i64 = 1;
    const CHANGED: i64 = 2;
    const ENDED: i64 = 4;

    pub fn swipes_from_scrolls() -> bool {
        NSEvent::isSwipeTrackingFromScrollEventsEnabled()
    }

    /// A two-finger gesture across at `at` (the window's points): the
    /// fingers down, `delta` points a step (positive: the fingers move
    /// right, as the content follows them), then up. A step each 16 ms, as
    /// a trackpad's come. Whether the window was there to take it.
    pub async fn gesture(at: (f32, f32), delta: f64) -> bool {
        let pause = |ms| zgx::timer::sleep(Duration::from_millis(ms));
        let mut phases = vec![(MAY_BEGIN, 0.), (BEGAN, delta)];
        phases.extend(std::iter::repeat_n((CHANGED, delta), 14));
        phases.push((ENDED, 0.));
        for (phase, delta) in phases {
            let Some((window, event)) = scroll(at, phase, delta) else {
                return false;
            };
            window.sendEvent(&event);
            pause(16).await;
        }
        true
    }

    /// A three-finger swipe across at `at`, `delta` to the right (negative:
    /// the left), as AppKit hands one to the view under it.
    pub fn three_finger_swipe(at: (f32, f32), delta: f64) {
        let Some((window, event)) = scroll(at, 0, delta * 10.) else {
            return;
        };
        let Some(view) = window.contentView() else {
            return;
        };
        // A swipe's `deltaX` is all the view reads of it; a scroll's has one.
        // SAFETY: `swipeWithEvent:`, which every NSResponder answers, with a
        // live event, on the main thread.
        unsafe {
            let _: () = objc2::msg_send![&*view, swipeWithEvent: &*event];
        }
    }

    /// A trackpad's scroll at `at` in the key window, at `phase` (0: none).
    fn scroll(
        at: (f32, f32),
        phase: i64,
        delta: f64,
    ) -> Option<(Retained<NSWindow>, Retained<NSEvent>)> {
        let mtm = MainThreadMarker::new().unwrap();
        let Some(window) = NSApplication::sharedApplication(mtm).keyWindow() else {
            println!("NO_KEY_WINDOW");
            return None;
        };
        // A scroll made from a CGEvent has no window, and its location in
        // one is where it is on the screen: put it where, read as the
        // window's, it is `at` (from the window's bottom left).
        let content = window.contentRectForFrameRect(window.frame());
        let main_height = NSScreen::screens(mtm)
            .firstObject()
            .map_or(0., |screen| screen.frame().size.height);
        let x = f64::from(at.0);
        let y = main_height - (content.size.height - f64::from(at.1));
        let event = CGEvent::new_scroll_wheel_event2(
            None,
            CGScrollEventUnit::Pixel,
            2,
            0,
            delta as i32,
            0,
        )?;
        let set = |field, value| CGEvent::set_integer_value_field(Some(&event), field, value);
        set(CGEventField::ScrollWheelEventIsContinuous, 1);
        set(CGEventField::ScrollWheelEventScrollPhase, phase);
        set(CGEventField::ScrollWheelEventMomentumPhase, 0);
        CGEvent::set_location(Some(&event), objc2_core_foundation::CGPoint::new(x, y));
        Some((window, NSEvent::eventWithCGEvent(&event)?))
    }
}
#[cfg(not(target_os = "macos"))]
mod trackpad {
    pub fn swipes_from_scrolls() -> bool {
        false
    }
    pub async fn gesture(_: (f32, f32), _: f64) -> bool {
        false
    }
    pub fn three_finger_swipe(_: (f32, f32), _: f64) {}
}
