//! Checks what macOS hands a window and what the window hands back (macOS),
//! by AppKit events posted to this process alone (the user's pointer and
//! keyboard are never used):
//!
//! - `CURSOR`: the cursor AppKit has (`+[NSCursor currentCursor]`) over
//!   boxes asking for each kind, and the arrow over none.
//! - `KEY`: the key a PC keyboard's Insert sends (the Help key, 0x72) as
//!   zgx hears it.
//! - `DRAG_ESCAPE`: a drag held over a destination, then Escape, then let go
//!   there: nothing dropped, the source told it was not taken, the drag over.
//!   A drag without Escape first, which drops (`DRAG_DROP`). Both stay inside
//!   the window: a drag carrying nothing to other apps never hands AppKit a
//!   session.
//! - `FILES`: two files, one name with a space, dragged in as Finder drags
//!   them (file URLs on a pasteboard of the probe's own, handed to the
//!   window's dragging destination as AppKit hands it a drag's): dropped on
//!   a box taking files, one batch of both paths, in order; dragged over it
//!   and out again, nothing; dragged over it and let go beside it, nothing.
//!   The pointer was last elsewhere: the drop lands where the files are.
//! - `TITLE_DRAG`: a press on a custom title bar's `window_drag` strip asks
//!   AppKit to move the window from that press (`-[NSWindow
//!   performWindowDragWithEvent:]`, recorded here and not run: the window
//!   server would follow the real pointer); a press on a button in the
//!   strip clicks it and moves nothing. The move itself follows the real
//!   pointer and is not checked.
//!
//! `scripts/macos_native_checks_probe.py` runs it and checks the lines.
use zgx::compose::prelude::*;
use zgx_desktop::{Application, Titlebar, WindowOptions};

/// What the drag carries: window-local, nothing for other apps.
#[derive(Clone)]
struct Chip;

/// Each cursor box: its kind's name, and the cursor it asks for.
const CURSORS: [(&str, Cursor); 5] = [
    ("pointer", Cursor::Pointer),
    ("text", Cursor::Text),
    ("move", Cursor::Move),
    ("not_allowed", Cursor::NotAllowed),
    ("col_resize", Cursor::ColResize),
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Application::new()
        .window(WindowOptions {
            title: "Checks".into(),
            width: 600.,
            height: 340.,
            titlebar: Titlebar::Inset,
            ..Default::default()
        })
        .run(|cx| {
            // The custom title bar: a 40-point strip, with a button at its right.
            let strip = row().w_full().h(40.).justify_end().window_drag().child(
                div()
                    .id("pin")
                    .w(60.)
                    .h(40.)
                    .focusable(true)
                    .bg(rgba(0x3d7ef733))
                    .on_click(|| println!("BUTTON pin")),
            );
            // Boxes at x = 10 + 90 i, 80 wide, from y = 50, 40 high.
            let cursors =
                CURSORS
                    .iter()
                    .fold(row().h(40.).gap(10.).px(10.), |boxes, (_, cursor)| {
                        boxes.child(div().w(80.).h(40.).bg(rgba(0x80808040)).cursor(*cursor))
                    });
            let chip = div()
                .w(120.)
                .h(28.)
                .bg(rgba(0x3d7ef766))
                .on_drag(|| Chip)
                .on_drag_end(|_: &Chip, accepted| println!("DRAG_END accepted={accepted}"));
            let target = div()
                .w(200.)
                .h(50.)
                .bg(rgba(0x40c08040))
                .on_drop(|_: &Chip, _| println!("DROPPED"));
            let files = div()
                .w(240.)
                .h(60.)
                .bg(rgba(0xc0804040))
                .on_files_drop(|paths, _| println!("FILES {paths:?}"));
            // Keys as zgx hears them, from the focused field up.
            let field = div()
                .id("keys")
                .w(240.)
                .h(30.)
                .focusable(true)
                .bg(rgba(0x80808020));
            cx.render(
                column()
                    .w_full()
                    .h_full()
                    .gap(10.)
                    .on_event(|cx| {
                        if cx.phase == zgx::input::EventPhase::Bubble
                            && let zgx::input::InputEvent::KeyDown { key, modifiers, .. } =
                                &cx.event
                        {
                            println!(
                                "KEY {key:?} shift={} control={} alt={} command={}",
                                modifiers.shift, modifiers.control, modifiers.alt, modifiers.meta
                            );
                        }
                    })
                    .child(strip)
                    .child(cursors)
                    .child(
                        row()
                            .px(10.)
                            .gap(160.)
                            .items_center()
                            .h(50.)
                            .child(chip)
                            .child(target),
                    )
                    .child(row().px(10.).gap(10.).child(files).child(field)),
            );
            #[cfg(target_os = "macos")]
            {
                let ui = cx.ui.downgrade();
                cx.tasks.spawn(async move {
                    macos::run(ui).await;
                    std::process::exit(0);
                });
            }
        })
}

#[cfg(target_os = "macos")]
mod macos {
    use objc2::runtime::{AnyObject, Imp, Sel};
    use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, msg_send, rc::Retained};
    use objc2_app_kit::{
        NSApplication, NSCursor, NSEvent, NSEventModifierFlags, NSEventType, NSPasteboard, NSWindow,
    };
    use objc2_foundation::{NSPoint, NSString};
    use std::time::Duration;
    use zgx_gpu::present_log::now_ns;

    thread_local! {
        static PRESSED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
        /// The synthetic pointer, bottom-up as AppKit's screens run.
        static POINTER: std::cell::Cell<Option<NSPoint>> = const { std::cell::Cell::new(None) };
        /// The window drags asked for: each event's type and place in its window.
        static DRAGS: std::cell::RefCell<Vec<(NSEventType, NSPoint)>> = const { std::cell::RefCell::new(Vec::new()) };
    }
    extern "C-unwind" fn pressed_buttons(
        _: *mut objc2::runtime::AnyObject,
        _: objc2::runtime::Sel,
    ) -> usize {
        PRESSED.with(|pressed| pressed.get())
    }
    extern "C-unwind" fn mouse_location(
        _: *mut objc2::runtime::AnyObject,
        _: objc2::runtime::Sel,
    ) -> NSPoint {
        POINTER
            .with(|pointer| pointer.get())
            .unwrap_or(NSPoint::new(0., 0.))
    }
    extern "C-unwind" fn window_drag(
        _: *mut objc2::runtime::AnyObject,
        _: objc2::runtime::Sel,
        event: *mut NSEvent,
    ) {
        // SAFETY: AppKit's caller passes a valid event or nil.
        let event = unsafe { event.as_ref() };
        DRAGS.with(|drags| {
            drags.borrow_mut().push(
                event.map_or((NSEventType(0), NSPoint::new(-1., -1.)), |event| {
                    (event.r#type(), event.locationInWindow())
                }),
            )
        });
    }
    /// Replace `class`'s (or its metaclass's) `selector` with `imp`, of the
    /// signature `types` states, in this process.
    ///
    /// # Safety
    /// `imp` must have the signature `types` gives, the method's own.
    unsafe fn replace(
        class: &objc2::runtime::AnyClass,
        selector: objc2::runtime::Sel,
        imp: objc2::runtime::Imp,
        types: &std::ffi::CStr,
    ) {
        // SAFETY: as the caller promises.
        unsafe {
            objc2::ffi::class_replaceMethod(
                class as *const _ as *mut _,
                selector,
                imp,
                types.as_ptr(),
            );
        }
    }
    /// `+[NSEvent pressedMouseButtons]` and `+[NSEvent mouseLocation]` answer
    /// the probe's presses and pointer; `-[NSWindow
    /// performWindowDragWithEvent:]` records its event and moves nothing.
    fn hook() {
        let event = objc2::runtime::AnyClass::get(c"NSEvent").unwrap();
        let window = objc2::runtime::AnyClass::get(c"NSWindow").unwrap();
        // SAFETY: each function has the replaced method's signature: a class
        // method returning NSUInteger, one returning NSPoint, an instance
        // method taking an object and returning nothing.
        unsafe {
            replace(
                event.metaclass(),
                objc2::sel!(pressedMouseButtons),
                std::mem::transmute::<extern "C-unwind" fn(*mut AnyObject, Sel) -> usize, Imp>(
                    pressed_buttons,
                ),
                c"Q@:",
            );
            replace(
                event.metaclass(),
                objc2::sel!(mouseLocation),
                std::mem::transmute::<extern "C-unwind" fn(*mut AnyObject, Sel) -> NSPoint, Imp>(
                    mouse_location,
                ),
                c"{CGPoint=dd}@:",
            );
            replace(
                window,
                objc2::sel!(performWindowDragWithEvent:),
                std::mem::transmute::<extern "C-unwind" fn(*mut AnyObject, Sel, *mut NSEvent), Imp>(
                    window_drag,
                ),
                c"v@:@",
            );
        }
    }
    async fn sleep(ms: u64) {
        zgx::timer::sleep(Duration::from_millis(ms)).await;
    }
    fn window() -> Retained<NSWindow> {
        let mtm = MainThreadMarker::new().unwrap();
        NSApplication::sharedApplication(mtm)
            .windows()
            .iter()
            .find(|window| window.isVisible() && window.title().to_string() == "Checks")
            .unwrap()
    }
    /// A point of the window's content, top-down, bottom-up in the window.
    fn local(window: &NSWindow, x: f64, y: f64) -> NSPoint {
        NSPoint::new(x, window.contentView().unwrap().frame().size.height - y)
    }
    /// The pointer over content point `(x, y)`, as the screen has it.
    fn point_at(window: &NSWindow, x: f64, y: f64) {
        let screen = window.convertPointToScreen(local(window, x, y));
        POINTER.with(|pointer| pointer.set(Some(screen)));
    }
    fn mouse(window: &NSWindow, kind: NSEventType, x: f64, y: f64, clicks: isize) {
        let mtm = MainThreadMarker::new().unwrap();
        point_at(window, x, y);
        let event = NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
            kind,
            local(window, x, y),
            NSEventModifierFlags::empty(),
            now_ns() as f64 / 1e9,
            window.windowNumber(),
            None,
            0,
            clicks,
            if kind == NSEventType::LeftMouseUp { 0. } else { 1. },
        )
        .unwrap();
        NSApplication::sharedApplication(mtm).postEvent_atStart(&event, false);
    }
    /// A key down then up, sent to the window (its first responder hears it).
    fn key(window: &NSWindow, code: u16, characters: &str) {
        for kind in [NSEventType::KeyDown, NSEventType::KeyUp] {
            let characters = NSString::from_str(characters);
            let event = NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
                kind,
                NSPoint::new(0., 0.),
                NSEventModifierFlags::empty(),
                now_ns() as f64 / 1e9,
                window.windowNumber(),
                None,
                &characters,
                &characters,
                false,
                code,
            )
            .unwrap();
            window.sendEvent(&event);
        }
    }
    async fn click(window: &NSWindow, x: f64, y: f64) {
        PRESSED.with(|pressed| pressed.set(1));
        mouse(window, NSEventType::LeftMouseDown, x, y, 1);
        sleep(60).await;
        mouse(window, NSEventType::LeftMouseUp, x, y, 1);
        PRESSED.with(|pressed| pressed.set(0));
        sleep(200).await;
    }

    /// The cursors the host sets for each box's kind (`native_host::set_cursor`).
    // The resize cursors' replacements need macOS 15, as the host's.
    #[allow(deprecated)]
    fn cursors() -> Vec<(&'static str, Retained<NSCursor>)> {
        vec![
            ("pointer", NSCursor::pointingHandCursor()),
            ("text", NSCursor::IBeamCursor()),
            ("move", NSCursor::openHandCursor()),
            ("not_allowed", NSCursor::operationNotAllowedCursor()),
            ("col_resize", NSCursor::resizeLeftRightCursor()),
        ]
    }
    async fn check_cursors(window: &NSWindow) {
        // The arrow over nothing first, then each box, then nothing again.
        let mut stops: Vec<(&str, f64, f64, Retained<NSCursor>)> =
            vec![("arrow", 560., 300., NSCursor::arrowCursor())];
        for (i, (name, cursor)) in cursors().into_iter().enumerate() {
            stops.push((name, 10. + 90. * i as f64 + 40., 70., cursor));
        }
        stops.push(("arrow", 560., 300., NSCursor::arrowCursor()));
        for (name, x, y, expected) in stops {
            mouse(window, NSEventType::MouseMoved, x, y, 0);
            sleep(250).await;
            let current = NSCursor::currentCursor();
            println!(
                "CURSOR {name} ok={}",
                Retained::as_ptr(&current) == Retained::as_ptr(&expected)
            );
        }
    }
    async fn drag(window: &NSWindow, ui: &zgx::widgets::WeakUi, escape: bool) {
        // The chip's centre (10 + 60, 100 + 25), then over the target's.
        let (from, to) = ((70., 125.), (390., 125.));
        PRESSED.with(|pressed| pressed.set(1));
        mouse(window, NSEventType::LeftMouseDown, from.0, from.1, 1);
        sleep(60).await;
        for step in 1..=30 {
            let t = step as f64 / 30.;
            mouse(
                window,
                NSEventType::LeftMouseDragged,
                from.0 + (to.0 - from.0) * t,
                from.1 + (to.1 - from.1) * t,
                1,
            );
            sleep(10).await;
        }
        sleep(150).await;
        let held = ui
            .upgrade()
            .is_some_and(|ui| ui.input.drag_state().is_some());
        if escape {
            key(window, 53, "\u{1b}");
            sleep(150).await;
        }
        let after = ui
            .upgrade()
            .is_some_and(|ui| ui.input.drag_state().is_some());
        mouse(window, NSEventType::LeftMouseUp, to.0, to.1, 1);
        PRESSED.with(|pressed| pressed.set(0));
        sleep(300).await;
        let label = if escape { "DRAG_ESCAPE" } else { "DRAG_DROP" };
        println!("{label} held={held} after={after}");
    }

    /// What AppKit hands a drag's destination (`NSDraggingInfo`), as a drag
    /// from Finder over the window would: its pasteboard (a private one,
    /// never the system's), where it is, and no source in this app.
    struct Info {
        pasteboard: Retained<NSPasteboard>,
        location: std::cell::Cell<NSPoint>,
    }
    objc2::define_class!(
        // SAFETY: NSObject has no subclassing requirements; the methods have
        // NSDraggingInfo's signatures, and only the probe calls them.
        #[unsafe(super(objc2_foundation::NSObject))]
        #[thread_kind = objc2::MainThreadOnly]
        #[name = "ZgxChecksDraggingInfo"]
        #[ivars = Info]
        struct DraggingInfo;
        impl DraggingInfo {
            #[unsafe(method_id(draggingPasteboard))]
            fn pasteboard(&self) -> Retained<NSPasteboard> {
                self.ivars().pasteboard.clone()
            }
            #[unsafe(method(draggingLocation))]
            fn location(&self) -> NSPoint {
                self.ivars().location.get()
            }
            #[unsafe(method(draggingSource))]
            fn source(&self) -> *mut objc2::runtime::AnyObject {
                std::ptr::null_mut()
            }
            #[unsafe(method(draggingSourceOperationMask))]
            fn operations(&self) -> usize {
                // NSDragOperationCopy | NSDragOperationLink | NSDragOperationGeneric, as Finder offers.
                1 | 2 | 4
            }
        }
    );
    /// How a files drag ends: let go over the files box, dragged out again,
    /// or let go beside it, over the keys field (which takes none).
    #[derive(Clone, Copy, Debug)]
    enum End {
        Dropped,
        Out,
        Beside,
    }
    async fn files(window: &NSWindow, end: End) {
        let mtm = MainThreadMarker::new().unwrap();
        let dir = std::env::temp_dir().join(format!("zgx-checks-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let paths = [dir.join("first file.txt"), dir.join("second.txt")];
        for path in &paths {
            std::fs::write(path, b"zgx").unwrap();
        }
        let pasteboard = NSPasteboard::pasteboardWithUniqueName();
        pasteboard.clearContents();
        let urls: Vec<
            Retained<objc2::runtime::ProtocolObject<dyn objc2_app_kit::NSPasteboardWriting>>,
        > = paths
            .iter()
            .map(|path| {
                objc2::runtime::ProtocolObject::from_retained(
                    objc2_foundation::NSURL::fileURLWithPath(&NSString::from_str(
                        &path.to_string_lossy(),
                    )),
                )
            })
            .collect();
        pasteboard.writeObjects(&objc2_foundation::NSArray::from_retained_slice(&urls));
        let info = DraggingInfo::alloc(mtm).set_ivars(Info {
            pasteboard: pasteboard.clone(),
            location: std::cell::Cell::new(NSPoint::new(0., 0.)),
        });
        // SAFETY: NSObject's `init`, on a fresh allocation with its ivars set.
        let info: Retained<DraggingInfo> = unsafe { msg_send![super(info), init] };
        let delegate = window.delegate().unwrap();
        let at = |x: f64, y: f64| info.ivars().location.set(local(window, x, y));
        // Over the files box (10..250, 160..220), then a little further in.
        at(60., 180.);
        // SAFETY: NSDraggingDestination's methods, as AppKit calls them,
        // with an object answering NSDraggingInfo's.
        let entered: usize = unsafe { msg_send![&*delegate, draggingEntered: &*info] };
        sleep(50).await;
        at(120., 190.);
        let updated: usize = unsafe { msg_send![&*delegate, draggingUpdated: &*info] };
        sleep(50).await;
        if let End::Beside = end {
            at(380., 175.);
            let _: usize = unsafe { msg_send![&*delegate, draggingUpdated: &*info] };
            sleep(50).await;
        }
        let taken = if let End::Out = end {
            let _: () = unsafe { msg_send![&*delegate, draggingExited: &*info] };
            false
        } else {
            let taken: bool = unsafe { msg_send![&*delegate, performDragOperation: &*info] };
            let _: () = unsafe { msg_send![&*delegate, concludeDragOperation: &*info] };
            taken
        };
        sleep(300).await;
        // SAFETY: `-[NSPasteboard releaseGlobally]`, which objc2 leaves
        // out: the private pasteboard goes with the probe's use of it.
        let _: () = unsafe { msg_send![&*pasteboard, releaseGlobally] };
        println!(
            "FILES_DRAG end={end:?} entered={entered} updated={updated} taken={taken} sent={:?}",
            paths
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    async fn title(window: &NSWindow) {
        let drags = || DRAGS.with(|drags| drags.borrow().clone());
        // The strip's empty middle (40 points high), then the button at its right.
        click(window, 300., 20.).await;
        let strip = drags();
        click(window, 570., 20.).await;
        let button = drags().len() - strip.len();
        let first = strip.first().map(|(kind, at)| {
            let content = window.contentView().unwrap().frame().size.height;
            (*kind == NSEventType::LeftMouseDown, at.x, content - at.y)
        });
        println!(
            "TITLE_DRAG strip={} down={} at={:?} button_drags={button}",
            strip.len(),
            first.is_some_and(|(down, ..)| down),
            first.map(|(_, x, y)| (x.round(), y.round()))
        );
    }

    pub async fn run(ui: zgx::widgets::WeakUi) {
        sleep(2500).await;
        hook();
        let window = window();
        check_cursors(&window).await;
        // The Insert key a PC keyboard sends a Mac: the Help key, its code
        // 0x72, its character NSHelpFunctionKey.
        click(&window, 380., 175.).await;
        key(&window, 0x72, "\u{F746}");
        sleep(200).await;
        drag(&window, &ui, false).await;
        drag(&window, &ui, true).await;
        files(&window, End::Dropped).await;
        files(&window, End::Out).await;
        files(&window, End::Beside).await;
        title(&window).await;
        println!("DONE");
    }
}
