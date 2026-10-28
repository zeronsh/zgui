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
    let appearance_fixture = std::env::args().any(|arg| arg == "--appearance-fixture");
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
            if appearance_fixture {
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

/// Native structure probe for dynamic Liquid Glass and appearance switching.
fn run_appearance_fixture(cx: &mut zgx_desktop::WindowContext) {
    #[cfg(target_os = "macos")]
    {
        use objc2::{MainThreadMarker, msg_send, rc::Retained, runtime::AnyClass};
        use objc2_app_kit::{NSAppearanceCustomization, NSApplication, NSView};
        use objc2_foundation::NSString;
        use zgx_desktop::{GlassBlend, GlassInnerShadow, GlassLayer};
        let window = cx.window.clone();
        cx.tasks.spawn(async move {
            let pause = || zgx::timer::sleep(std::time::Duration::from_millis(100));
            pause().await;
            let mtm = MainThreadMarker::new().unwrap();
            let ns = NSApplication::sharedApplication(mtm)
                .windows()
                .into_iter()
                .find(|w| w.title().to_string() == TITLE)
                .unwrap();
            let content = ns.contentView().unwrap();
            // SAFETY: the live native content view's frame view.
            let frame = unsafe { content.superview() }.unwrap();
            let effects = || {
                frame
                    .subviews()
                    .into_iter()
                    .filter(|view| {
                        // SAFETY: NSView's public identifier getter.
                        let id: Option<Retained<NSString>> =
                            unsafe { msg_send![&**view, identifier] };
                        id.is_some_and(|id| id.to_string() == "zgx.window.glass")
                    })
                    .collect::<Vec<_>>()
            };
            let mut glass = Glass::new(GlassMaterial::Liquid);
            glass.tint = Some(0x14151780);
            glass.corner_radius = Some(12.);
            glass.backdrop = Some(GlassLayer {
                color: 0x24252880,
                blend: GlassBlend::Color,
            });
            glass.fill = Some(GlassLayer {
                color: 0x0000004d,
                blend: GlassBlend::Overlay,
            });
            glass.inner_shadow = Some(GlassInnerShadow {
                color: 0xffffff99,
                blur: 2.5,
                x: 0.,
                y: 1.5,
                blend: GlassBlend::Overlay,
                knockout: true,
                geometry: Default::default(),
            });
            for theme in [
                Some(zgx::theme::Appearance::Dark),
                Some(zgx::theme::Appearance::Light),
                None,
            ] {
                window.set_theme(theme);
                window.set_glass(Some(glass));
                pause().await;
                let views = effects();
                assert_eq!(views.len(), 1, "a switch must replace the native effect");
                let group = &views[0];
                assert_eq!(
                    group.class().name().to_string_lossy(),
                    "ZgxWindowGlassContent"
                );
                let surface = group.subviews().objectAtIndex(0);
                if AnyClass::get(c"NSGlassEffectView").is_some() {
                    assert_eq!(
                        surface.class().name().to_string_lossy(),
                        "NSGlassEffectContainerView"
                    );
                    // SAFETY: public macOS 26 native glass properties and NSView.
                    unsafe {
                        let effect: Retained<NSView> = msg_send![&*surface, contentView];
                        let style: isize = msg_send![&*effect, style];
                        let radius: f64 = msg_send![&*effect, cornerRadius];
                        assert_eq!((style, radius), (0, 12.));
                        let overlays: Retained<NSView> = msg_send![&*effect, contentView];
                        assert_eq!(overlays.subviews().len(), 2);
                        for overlay in overlays.subviews() {
                            let layer = overlay.layer().unwrap();
                            let filter: Option<Retained<objc2::runtime::AnyObject>> =
                                msg_send![&*layer, compositingFilter];
                            assert!(filter.is_some(), "native blend filter installed");
                        }
                    }
                } else {
                    assert_eq!(surface.class().name().to_string_lossy(), "ZgxWindowGlass");
                }
                let inset = group.subviews().objectAtIndex(1);
                assert_eq!(
                    inset.class().name().to_string_lossy(),
                    "ZgxWindowInnerShadow"
                );
                let root = inset.layer().unwrap();
                // SAFETY: these are the CALayers installed by our own
                // resource shadow view, retained on the UI thread.
                let shape = unsafe { root.sublayers() }.unwrap().objectAtIndex(0);
                let shadow = unsafe { shape.sublayers() }.unwrap().objectAtIndex(0);
                assert_eq!(shadow.shadowRadius(), 2.5);
                assert_eq!(
                    shadow.shadowOffset(),
                    objc2_foundation::NSSize::new(0., -1.5)
                );
                assert!(shadow.shadowPath().is_some());
                assert!(
                    !root.masksToBounds(),
                    "resource surface clipping remains disabled"
                );
                assert!(
                    shape.masksToBounds(),
                    "inner shadow clips only to its own shape"
                );
                let mut resized = group.frame().size;
                resized.width += 20.;
                group.setFrameSize(resized);
                let inset = group.subviews().objectAtIndex(1);
                // SAFETY: the same owned shadow tree after its resize.
                let shape = unsafe { inset.layer().unwrap().sublayers() }
                    .unwrap()
                    .objectAtIndex(0);
                assert_eq!(shape.frame().size.width, resized.width);
                assert_eq!(ns.appearance().is_some(), theme.is_some());
                println!("APPEARANCE glass {theme:?} one native effect, resource blend layers");
                window.set_glass(None);
                pause().await;
                assert!(effects().is_empty(), "opaque removes the native effect");
                println!("APPEARANCE opaque {theme:?} no native effect");
            }
            println!("APPEARANCE PASS");
            window.close();
        });
    }
    #[cfg(not(target_os = "macos"))]
    cx.window.close();
}
