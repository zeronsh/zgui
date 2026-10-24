//! Frosted-glass counter with a custom, comet-style inset titlebar.
//! Run: cargo run --release -p zgx-desktop --example glass_counter
//!
//! macOS: the window keeps its native traffic lights over a transparent,
//! full-size-content titlebar, and the desktop behind it is blurred. Other
//! platforms fall back to a frameless translucent window with drawn controls.
//!
//! `-- --fixture`: checks the chrome `WindowOptions` asked for against live
//! AppKit state, prints `FIXTURE` lines, then exits (non-zero on failure).
use zgx::{
    compose::prelude::*,
    input::{EventPhase, InputEvent, Key},
    text_layout::{FontFeatures, TextAlign},
};
use zgx_desktop::{Application, Glass, Titlebar, WindowOptions};

const TITLE: &str = "zgx glass counter";
const WIDTH: f64 = 340.;
const HEIGHT: f64 = 420.;
const TITLEBAR: f32 = 40.;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = std::env::args().any(|arg| arg == "--fixture");
    Application::new()
        .window(WindowOptions {
            title: TITLE.into(),
            width: WIDTH,
            height: HEIGHT,
            transparent: true,
            decorations: cfg!(target_os = "macos"),
            min_size: Some((280., 340.)),
            // Content under inset traffic lights, on frosted glass.
            titlebar: Titlebar::Inset,
            glass: Some(Glass::default()),
            ..Default::default()
        })
        .run(move |cx| {
            if fixture {
                run_fixture(cx);
            }
            let view = cx.render(component(move |cx| {
                let count = cx.state(0_i64);
                let (inc, dec, reset, keys, label, tint) = (
                    count.clone(),
                    count.clone(),
                    count.clone(),
                    count.clone(),
                    count.clone(),
                    count.clone(),
                );
                // Untitled drag strip under the traffic lights.
                let titlebar = div().w_full().h(TITLEBAR).window_drag();
                let number = text_signal(move || label.get().to_string())
                    .text_size(104.)
                    .font_weight(200)
                    .letter_spacing(-3.)
                    .font_features(FontFeatures::new([(*b"tnum", 1)]))
                    .text_align(TextAlign::Center)
                    .reactive_style(move || {
                        Styles::new().text_color(match tint.get() {
                            n if n > 0 => rgb(0xb4f5d8),
                            n if n < 0 => rgb(0xffc2b8),
                            _ => rgb(0xffffff),
                        })
                    });
                let stepper = row()
                    .p(5.)
                    .gap(4.)
                    .items_center()
                    .rounded(30.)
                    .bg(rgba(0xffffff12))
                    .border(1.)
                    .border_color(rgba(0xffffff22))
                    .child(step_button(false).on_click(move || {
                        dec.update(|n| *n -= 1);
                    }))
                    .child(div().w(1.).h(22.).bg(rgba(0xffffff26)))
                    .child(step_button(true).on_click(move || {
                        inc.update(|n| *n += 1);
                    }));
                column()
                    .w_full()
                    .h_full()
                    .bg(rgba(0x0c101860))
                    .rounded(if cfg!(target_os = "macos") { 0. } else { 14. })
                    .focusable(true)
                    .on_event(move |event| {
                        if event.phase == EventPhase::Bubble {
                            return;
                        }
                        let InputEvent::KeyDown { key, .. } = &event.event else {
                            return;
                        };
                        match key {
                            Key::Character(c) if c == "+" || c == "=" => {
                                keys.update(|n| *n += 1);
                            }
                            Key::Character(c) if c == "-" => {
                                keys.update(|n| *n -= 1);
                            }
                            Key::Character(c) if c == "0" => {
                                keys.set(0);
                            }
                            _ => return,
                        }
                        event.prevent_default();
                    })
                    .child(titlebar)
                    .child(
                        column()
                            .w_full()
                            .grow()
                            .items_center()
                            .justify_center()
                            .gap(6.)
                            .pb(TITLEBAR)
                            .child(
                                text("COUNT")
                                    .text_size(11.)
                                    .font_weight(600)
                                    .letter_spacing(2.4)
                                    .text_color(rgba(0xffffff70)),
                            )
                            .child(row().w_full().justify_center().child(number))
                            .child(div().h(22.))
                            .child(stepper)
                            .child(div().h(14.))
                            .child(
                                button()
                                    .px(14.)
                                    .py(6.)
                                    .rounded(999.)
                                    .bg(rgba(0xffffff00))
                                    .text_size(12.)
                                    .font_weight(500)
                                    .text_color(rgba(0xffffff80))
                                    .hover(|s| s.bg(rgba(0xffffff12)).text_color(rgb(0xffffff)))
                                    .active(|s| s.bg(rgba(0xffffff20)))
                                    .focus(|s| s)
                                    .child("Reset")
                                    .on_click(move || {
                                        reset.set(0);
                                    }),
                            ),
                    )
            }));
            cx.ui.input.focus(&cx.ui.scene, Some(view.node()));
        })
}

fn step_button(plus: bool) -> View {
    // Drawn glyphs: the font's minus sits on the baseline, not the plus's centre.
    let bar = |w: f32, h: f32| div().absolute().size(w, h).rounded(1.).bg(rgb(0xffffff));
    let mut icon = overlay().size(16., 16.).child(bar(16., 2.).mt(7.));
    if plus {
        // Placed from the left either way: a margin would place it from the
        // right, right to left (an absolute box starts where its line does).
        icon = icon.child(bar(2., 16.).left(7.));
    }
    button()
        .size(76., 48.)
        .rounded(24.)
        .items_center()
        .justify_center()
        .bg(rgba(0xffffff00))
        .hover(|s| s.bg(rgba(0xffffff18)))
        .active(|s| s.bg(rgba(0xffffff2c)))
        // zgx focuses buttons on click and has no focus-visible; no ring.
        .focus(|s| s)
        .child(icon)
}

/// Checks against live AppKit state (see the module documentation).
fn run_fixture(cx: &mut zgx_desktop::WindowContext) {
    #[cfg(target_os = "macos")]
    {
        use objc2::{MainThreadMarker, msg_send, runtime::AnyObject, sel};
        use objc2_app_kit::{NSApplication, NSWindowStyleMask, NSWindowTitleVisibility};
        let radius = cx.window_info.corner_radius.clone();
        let window = cx.window.clone();
        cx.tasks.spawn(async move {
            zgx::timer::sleep(std::time::Duration::from_millis(600)).await;
            let mut failures = 0;
            let mut check = |name: &str, ok: bool, detail: String| {
                println!(
                    "FIXTURE {} {name}: {detail}",
                    if ok { "ok" } else { "FAIL" }
                );
                failures += usize::from(!ok);
            };
            let mtm = MainThreadMarker::new().expect("the UI thread");
            let found = NSApplication::sharedApplication(mtm)
                .windows()
                .into_iter()
                .find(|w| w.title().to_string() == TITLE);
            check("window", found.is_some(), String::new());
            let Some(ns) = found else {
                std::process::exit(1);
            };
            check(
                "content under the title bar",
                ns.styleMask()
                    .contains(NSWindowStyleMask::FullSizeContentView)
                    && ns.titlebarAppearsTransparent()
                    && ns.titleVisibility() == NSWindowTitleVisibility::Hidden,
                format!("{:?}", ns.styleMask()),
            );
            // SAFETY: NSWindow's `toolbar` (an unretained NSToolbar or nil,
            // only compared with null) and `toolbarStyle` (an NSInteger).
            let (toolbar, toolbar_style): (*mut AnyObject, isize) =
                unsafe { (msg_send![&*ns, toolbar], msg_send![&*ns, toolbarStyle]) };
            check(
                "traffic lights inset by a compact unified toolbar",
                !toolbar.is_null() && toolbar_style == 4,
                format!("style {toolbar_style}"),
            );
            let content = ns.contentView().expect("a content view");
            let size = content.frame().size;
            check(
                "content keeps the size asked for",
                (size.width, size.height) == (WIDTH, HEIGHT),
                format!("{size:?}"),
            );
            let corner: f64 = if ns.class().responds_to(sel!(_cornerRadius)) {
                // SAFETY: an unpublished getter taking nothing, sent only where
                // it exists; a CGFloat as zgx's window chrome reads it.
                unsafe { msg_send![&*ns, _cornerRadius] }
            } else {
                10.
            };
            check(
                "WindowInfo::corner_radius",
                radius.get() == corner && corner > 0.,
                format!("{} vs {corner}", radius.get()),
            );
            // SAFETY: the window's frame view, which the window keeps while it
            // holds its content view.
            let frame = unsafe { content.superview() }.expect("a frame view");
            let subviews = frame.subviews();
            let glass = subviews
                .iter()
                .position(|view| view.class().name().to_string_lossy() == "ZgxWindowGlass");
            let below = glass.zip(
                subviews
                    .iter()
                    .position(|view| std::ptr::eq(&*view, &*content)),
            );
            check(
                "glass under the content",
                below.is_some_and(|(glass, content)| glass < content),
                format!("{below:?}"),
            );
            if let Some(index) = glass {
                let view = subviews.objectAtIndex(index);
                // SAFETY: NSVisualEffectView's getters (it is zgx's glass view),
                // each an NSInteger enum.
                let (material, blending, state): (isize, isize, isize) = unsafe {
                    (
                        msg_send![&*view, material],
                        msg_send![&*view, blendingMode],
                        msg_send![&*view, state],
                    )
                };
                check(
                    "HUD material, behind the window, always active",
                    (material, blending, state) == (13, 0, 1),
                    format!("{material} {blending} {state}"),
                );
                // SAFETY: NSView's `layer`, unretained (the view keeps it) and
                // nil-checked; CALayer's `cornerRadius` (CGFloat) and
                // `masksToBounds` (BOOL).
                let layer: *mut AnyObject = unsafe { msg_send![&*view, layer] };
                let rounded = unsafe { layer.as_ref() }.map(|layer| unsafe {
                    let r: f64 = msg_send![layer, cornerRadius];
                    let masks: bool = msg_send![layer, masksToBounds];
                    (r, masks)
                });
                check(
                    "cut to the window's corners",
                    rounded == Some((corner, true)),
                    format!("{rounded:?}"),
                );
            }
            check(
                "window buttons shown",
                ["close", "minimize", "zoom"]
                    .iter()
                    .enumerate()
                    // SAFETY: `-standardWindowButton:` takes an NSWindowButton
                    // (NSUInteger) and returns an unretained button or nil,
                    // which the window keeps; `isHidden` returns BOOL.
                    .all(|(i, _)| unsafe {
                        let button: *mut AnyObject = msg_send![&*ns, standardWindowButton: i];
                        button.as_ref().is_some_and(|b| !msg_send![b, isHidden])
                    }),
                String::new(),
            );
            println!("FIXTURE {failures} failures");
            window.close();
            if failures > 0 {
                std::process::exit(1);
            }
        });
    }
    #[cfg(not(target_os = "macos"))]
    let _ = cx;
}
