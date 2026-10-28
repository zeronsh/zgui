//! Frosted-glass counter with a custom, comet-style inset titlebar.
//! Run: cargo run --release -p zgx-desktop --example glass_counter
//!
//! macOS: the window keeps its native traffic lights over a transparent,
//! full-size-content titlebar, and the desktop behind it is blurred. Other
//! platforms fall back to a frameless translucent window with drawn controls.
//!
//! `-- --fixture`: checks the chrome `WindowOptions` asked for against live
//! AppKit state, prints `FIXTURE` lines, then exits (non-zero on failure).
//! `ZGX_GLASS_MATERIAL=window` or `titlebar` frosts the window with that
//! material instead of the heads-up glass (`windows_glass_smoke.py`).
//! `-- --appearance-fixture` (macOS, Windows): replaces, takes away and
//! gives back the glass (`WindowHandle::set_glass`) and shows the window
//! light, dark and as the system does (`set_theme`), checking AppKit's
//! views and appearance (Windows: the DWM backdrop and dark mode) and zgx's
//! theme each time; `APPEARANCE` lines, then it exits. With `--decorated`
//! the window has the platform's own title bar, which follows the theme.
use zgx::{
    compose::prelude::*,
    input::{EventPhase, InputEvent, Key},
    text_layout::{FontFeatures, TextAlign},
};
use zgx_desktop::{Application, Glass, GlassMaterial, Titlebar, WindowOptions};

const TITLE: &str = "zgx glass counter";
const WIDTH: f64 = 340.;
const HEIGHT: f64 = 420.;
const TITLEBAR: f32 = 40.;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = std::env::args().any(|arg| arg == "--fixture");
    let appearance = std::env::args().any(|arg| arg == "--appearance-fixture");
    // `--decorated`: the platform's own title bar, as a standard window has
    // it (on Windows, to see it follow `set_theme`).
    let decorated = std::env::args().any(|arg| arg == "--decorated");
    Application::new()
        .window(WindowOptions {
            title: TITLE.into(),
            width: WIDTH,
            height: HEIGHT,
            transparent: true,
            decorations: decorated || cfg!(target_os = "macos"),
            min_size: Some((280., 340.)),
            // Content under inset traffic lights, on frosted glass.
            titlebar: if decorated {
                Titlebar::Standard
            } else {
                Titlebar::Inset
            },
            glass: Some(match std::env::var("ZGX_GLASS_MATERIAL").as_deref() {
                Ok("window") => Glass::new(GlassMaterial::WindowBackground),
                Ok("titlebar") => Glass::new(GlassMaterial::Titlebar),
                _ => Glass::default(),
            }),
            ..Default::default()
        })
        .run(move |cx| {
            if fixture {
                run_fixture(cx);
            }
            if appearance {
                run_appearance_fixture(cx);
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

/// `--appearance-fixture`: the glass and appearance a window is given while
/// it shows, as AppKit has them, and the theme zgx follows.
fn run_appearance_fixture(cx: &mut zgx_desktop::WindowContext) {
    #[cfg(target_os = "macos")]
    {
        use objc2::MainThreadMarker;
        use objc2_app_kit::{NSApplication, NSVisualEffectMaterial, NSVisualEffectView};
        use zgx::theme::Appearance;
        let window = cx.window.clone();
        let themes = cx.ui.theme().clone();
        cx.tasks.spawn(async move {
            let pause = || zgx::timer::sleep(std::time::Duration::from_millis(400));
            pause().await;
            let mtm = MainThreadMarker::new().expect("the UI thread");
            let Some(ns) = NSApplication::sharedApplication(mtm)
                .windows()
                .into_iter()
                .find(|w| w.title().to_string() == TITLE)
            else {
                println!("APPEARANCE FAIL window");
                std::process::exit(1);
            };
            // The effect views zgx put under the content, by material.
            let glass = || -> Vec<isize> {
                // SAFETY: the content view's superview, the window's frame
                // view, which the window keeps while it holds its content.
                let Some(frame) = ns.contentView().and_then(|c| unsafe { c.superview() }) else {
                    return Vec::new();
                };
                frame
                    .subviews()
                    .into_iter()
                    .filter(|view| view.class().name().to_bytes() == b"ZgxWindowGlass")
                    .filter_map(|view| view.downcast::<NSVisualEffectView>().ok())
                    .map(|view| view.material().0)
                    .collect()
            };
            let appearance = || {
                // SAFETY: NSWindow's `appearance` (an NSAppearance or nil)
                // and NSAppearance's `name` (an NSString), retained here.
                unsafe {
                    let appearance: Option<objc2::rc::Retained<objc2::runtime::AnyObject>> =
                        objc2::msg_send![&*ns, appearance];
                    appearance.map(|appearance| {
                        let name: objc2::rc::Retained<objc2_foundation::NSString> =
                            objc2::msg_send![&*appearance, name];
                        name.to_string()
                    })
                }
            };
            let mut failures = 0;
            let mut check = |name: &str, ok: bool, detail: String| {
                println!(
                    "APPEARANCE {} {name}: {detail}",
                    if ok { "ok" } else { "FAIL" }
                );
                failures += usize::from(!ok);
            };
            let hud = NSVisualEffectMaterial::HUDWindow.0;
            let popover = NSVisualEffectMaterial::Popover.0;
            check(
                "starts with its glass",
                glass() == [hud],
                format!("{:?}", glass()),
            );
            window.set_glass(Some(Glass::new(GlassMaterial::Popover)));
            pause().await;
            check(
                "glass replaced, not stacked",
                glass() == [popover],
                format!("{:?}", glass()),
            );
            window.set_glass(None);
            pause().await;
            check(
                "glass taken away",
                glass().is_empty(),
                format!("{:?}", glass()),
            );
            window.set_glass(Some(Glass::default()));
            pause().await;
            check(
                "glass given back",
                glass() == [hud],
                format!("{:?}", glass()),
            );
            for (theme, name) in [
                (Appearance::Light, "NSAppearanceNameAqua"),
                (Appearance::Dark, "NSAppearanceNameDarkAqua"),
            ] {
                window.set_theme(Some(theme));
                pause().await;
                check(
                    &format!("{theme:?}"),
                    appearance().as_deref() == Some(name) && themes.system_appearance() == theme,
                    format!("{:?} zgx={:?}", appearance(), themes.system_appearance()),
                );
            }
            window.set_theme(None);
            pause().await;
            check(
                "the system's again",
                appearance().is_none(),
                format!("{:?}", appearance()),
            );
            println!(
                "APPEARANCE {}",
                if failures == 0 { "PASS" } else { "FAILED" }
            );
            std::process::exit(i32::from(failures > 0));
        });
    }
    #[cfg(windows)]
    {
        use windows::{
            Win32::{
                Foundation::HWND,
                Graphics::Dwm::{
                    DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_USE_IMMERSIVE_DARK_MODE, DwmGetWindowAttribute,
                },
                UI::WindowsAndMessaging::FindWindowW,
            },
            core::HSTRING,
        };
        use zgx::theme::Appearance;
        let window = cx.window.clone();
        let themes = cx.ui.theme().clone();
        cx.tasks.spawn(async move {
            let pause = || zgx::timer::sleep(std::time::Duration::from_millis(500));
            pause().await;
            // SAFETY: looks up a top-level window of this title.
            let Ok(hwnd) = (unsafe { FindWindowW(None, &HSTRING::from(TITLE)) }) else {
                println!("APPEARANCE FAIL window");
                std::process::exit(1);
            };
            // A DWM attribute of the window, as an integer.
            let attribute = |hwnd: HWND, attribute| {
                let mut value = 0_i32;
                // SAFETY: reads one documented attribute of our own window
                // into a local of its size.
                let read = unsafe {
                    DwmGetWindowAttribute(
                        hwnd,
                        attribute,
                        &mut value as *mut _ as *mut _,
                        size_of_val(&value) as u32,
                    )
                };
                read.ok().map(|()| value)
            };
            let backdrop = || attribute(hwnd, DWMWA_SYSTEMBACKDROP_TYPE);
            let dark = || attribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE);
            let mut failures = 0;
            let mut check = |name: &str, ok: bool, detail: String| {
                println!(
                    "APPEARANCE {} {name}: {detail}",
                    if ok { "ok" } else { "FAIL" }
                );
                failures += usize::from(!ok);
            };
            // DWMSBT_NONE 1, DWMSBT_MAINWINDOW (Mica) 2, DWMSBT_TRANSIENTWINDOW (Acrylic) 3.
            check(
                "starts with Acrylic",
                backdrop() == Some(3),
                format!("{:?}", backdrop()),
            );
            window.set_glass(Some(Glass::new(GlassMaterial::WindowBackground)));
            pause().await;
            check(
                "replaced by Mica",
                backdrop() == Some(2),
                format!("{:?}", backdrop()),
            );
            window.set_glass(None);
            pause().await;
            check(
                "taken away",
                backdrop() == Some(1),
                format!("{:?}", backdrop()),
            );
            window.set_glass(Some(Glass::default()));
            pause().await;
            check(
                "given back",
                backdrop() == Some(3),
                format!("{:?}", backdrop()),
            );
            for (theme, want) in [(Appearance::Light, 0), (Appearance::Dark, 1)] {
                window.set_theme(Some(theme));
                pause().await;
                check(
                    &format!("{theme:?}"),
                    dark() == Some(want) && themes.system_appearance() == theme,
                    format!("dark={:?} zgx={:?}", dark(), themes.system_appearance()),
                );
            }
            window.set_theme(None);
            pause().await;
            println!(
                "APPEARANCE ok the system's again: dark={:?} zgx={:?}",
                dark(),
                themes.system_appearance()
            );
            println!(
                "APPEARANCE {}",
                if failures == 0 { "PASS" } else { "FAILED" }
            );
            std::process::exit(i32::from(failures > 0));
        });
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    cx.window.close();
}
