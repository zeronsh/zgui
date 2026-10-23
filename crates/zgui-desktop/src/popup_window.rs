//! Popups past the window's edges, as AppKit shows menus and popovers.
//!
//! An anchored popup (popover, menu, submenu, context menu, tooltip) is a
//! popup root of its window's scene (`Scene::set_popup`). It stays in that
//! scene, with its input, focus and dismissal. The window's renderer leaves
//! it out, and a borderless, transparent, non-activating child panel shows
//! it instead, drawn by a renderer of its own (`GpuRenderer::set_popup`)
//! that shares the window's device and text caches. The parent window stays
//! key, so keys reach the popup's content through the window's focus. The
//! panel's pointer input goes to the window's host, in window coordinates.
//!
//! Panels are pooled per process and reused hidden: making a window and first
//! ordering it front costs the window server about 100 ms, and a menu must
//! open within a frame. None is made before the pointer first reaches
//! something that opens a popup (`Ui::on_popup_soon`): an app that never
//! opens one has none. A hidden panel draws nothing and never wakes. A
//! closing popup fades out, as AppKit's menus do.
//!
//! Elsewhere, popups stay inside their window (the host gives no screens to
//! show them on, `Ui::set_popup_screens`).
//! todo(windows): the same `PopupHost` over owned popup windows:
//! - `CreateWindowExW(WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW |
//!   WS_EX_NOREDIRECTIONBITMAP, WS_POPUP)` owned by the parent `HWND` (above
//!   it, hidden with it), shown with `SW_SHOWNOACTIVATE`, answering
//!   `WM_MOUSEACTIVATE` with `MA_NOACTIVATE` so the parent keeps the keyboard;
//! - its frames through a DirectComposition visual, as the window's overlay
//!   plane presents (`GpuRenderer::set_popup` as here);
//! - DWM's rounded corners and shadow (`DWMWA_WINDOW_CORNER_PREFERENCE`), and
//!   a menu's acrylic behind a translucent panel
//!   (`DWMWA_SYSTEMBACKDROP_TYPE = DWMSBT_TRANSIENTWINDOW`);
//! - pointer input posted to the parent's host in its coordinates, as here;
//! - screens from `EnumDisplayMonitors` (`rcWork`, `GetDpiForMonitor`);
//! - an owned window does not follow its owner: move it on the owner's
//!   `WM_WINDOWPOSCHANGED`; close popups on `WM_ACTIVATEAPP` (false) and on
//!   a press in the owner's non-client area (`WM_NCLBUTTONDOWN`).
//!
//! X11 (`x11`): override-redirect ARGB windows typed as popup menus
//! (`_NET_WM_WINDOW_TYPE_POPUP_MENU`), placed by the app on the monitors
//! winit reports, dismissed with the parent's focus. winit makes them, so
//! they are made where the event loop is (`make`, as the pointer nears a
//! trigger, or the frame after a popup found none ready).
//! todo(wayland): `xdg_popup` with an `xdg_positioner` (the compositor places,
//! flips and dismisses it; the app cannot learn screen bounds), which winit
//! does not expose.
use winit::event::{MouseButton, MouseScrollDelta, TouchPhase};

/// Pointer input over a popup window, in its parent window's coordinates.
#[derive(Clone, Copy, Debug)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) enum PopupInput {
    Moved {
        x: f32,
        y: f32,
    },
    Button {
        x: f32,
        y: f32,
        button: MouseButton,
        pressed: bool,
    },
    Wheel {
        x: f32,
        y: f32,
        delta: MouseScrollDelta,
        phase: TouchPhase,
    },
    /// Out of the popup, and not into its parent window.
    Left,
    /// A press elsewhere in the parent window (its title bar): popups close.
    Dismiss,
}

#[cfg(target_os = "macos")]
pub(crate) use macos::{PopupHost, enabled, popup_screens, warm};

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(crate) use other::{PopupHost, enabled, popup_screens, warm};

#[cfg(target_os = "linux")]
pub(crate) use x11::{PopupHost, enabled, make, pointer, popup_screens, route, wanted, warm};

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
mod other {
    use super::PopupInput;
    use zgui::scene::{Rect, Scene};
    /// Popups stay inside their window here (see the module's notes).
    #[derive(Default)]
    pub(crate) struct PopupHost;
    impl PopupHost {
        pub fn new(_: impl Fn(PopupInput) + 'static) -> Self {
            Self
        }
        pub fn showing(&self) -> bool {
            false
        }
        #[allow(clippy::too_many_arguments)]
        pub fn update(
            &mut self,
            _: &winit::window::Window,
            _: &Scene,
            _: &zgui::semantics::Semantics,
            _: &zgui_gpu::GpuContext,
            _: f32,
            _: bool,
        ) -> Result<(), zgui_gpu::GpuError> {
            Ok(())
        }
        pub fn hide_all(&mut self, _: &winit::window::Window) {}
    }
    pub(crate) fn popup_screens(_: &winit::window::Window) -> Vec<Rect> {
        Vec::new()
    }
    pub(crate) fn enabled() -> bool {
        false
    }
    pub(crate) fn warm(_: &zgui_gpu::GpuContext, _: f32) {}
}

#[cfg(target_os = "macos")]
mod macos {
    use super::PopupInput;
    use objc2::{
        DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained,
        runtime::AnyObject,
    };
    use objc2_app_kit::{
        NSAutoresizingMaskOptions, NSBackingStoreType, NSColor, NSEvent, NSEventPhase, NSPanel,
        NSPopUpMenuWindowLevel, NSTrackingArea, NSTrackingAreaOptions, NSView,
        NSVisualEffectBlendingMode, NSVisualEffectMaterial, NSVisualEffectState,
        NSVisualEffectView, NSWindow, NSWindowAnimationBehavior, NSWindowCollectionBehavior,
        NSWindowOrderingMode, NSWindowStyleMask,
    };
    use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
    use objc2_quartz_core::{CALayer, CATransaction};
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };
    use winit::{
        dpi::{LogicalPosition, PhysicalPosition},
        event::{MouseButton, MouseScrollDelta, TouchPhase},
    };
    use zgui::{
        scene::{NodeId, NodeKind, Rect, Scene},
        semantics::{Role, Semantics},
    };
    use zgui_gpu::{GpuContext, GpuError, GpuRenderer};

    /// How many panels stay made while hidden: a menu, its submenu and a
    /// tooltip open together without making one. `ZGUI_POPUP_POOL` sets
    /// another number (to measure what each costs).
    fn pool_size() -> usize {
        thread_local! {
            static SIZE: usize = std::env::var("ZGUI_POPUP_POOL")
                .ok()
                .and_then(|size| size.parse().ok())
                .unwrap_or(3);
        }
        SIZE.with(|size| *size)
    }

    /// What hands a popup's input to its parent window's host.
    type Sink = Rc<dyn Fn(PopupInput)>;
    /// Where a panel's input goes: its popup's origin in its parent window,
    /// and the parent's host.
    #[derive(Default)]
    pub(crate) struct Target {
        origin: Cell<(f32, f32)>,
        send: RefCell<Option<Sink>>,
        parent: RefCell<Option<Retained<NSWindow>>>,
    }
    define_class!(
        #[unsafe(super(NSView))]
        #[thread_kind = MainThreadOnly]
        #[name = "ZguiPopupView"]
        #[ivars = Target]
        pub(crate) struct PopupView;
        impl PopupView {
            #[unsafe(method(isFlipped))]
            fn is_flipped(&self) -> bool {
                true
            }
            // The first click on a popup acts, as on a menu.
            #[unsafe(method(acceptsFirstMouse:))]
            fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
                true
            }
            #[unsafe(method(mouseDown:))]
            fn mouse_down(&self, event: &NSEvent) {
                self.button(event, MouseButton::Left, true);
            }
            #[unsafe(method(mouseUp:))]
            fn mouse_up(&self, event: &NSEvent) {
                self.button(event, MouseButton::Left, false);
            }
            #[unsafe(method(rightMouseDown:))]
            fn right_mouse_down(&self, event: &NSEvent) {
                self.button(event, MouseButton::Right, true);
            }
            #[unsafe(method(rightMouseUp:))]
            fn right_mouse_up(&self, event: &NSEvent) {
                self.button(event, MouseButton::Right, false);
            }
            #[unsafe(method(otherMouseDown:))]
            fn other_mouse_down(&self, event: &NSEvent) {
                self.button(event, other_button(event), true);
            }
            #[unsafe(method(otherMouseUp:))]
            fn other_mouse_up(&self, event: &NSEvent) {
                self.button(event, other_button(event), false);
            }
            #[unsafe(method(mouseMoved:))]
            fn mouse_moved(&self, event: &NSEvent) {
                self.moved(event);
            }
            #[unsafe(method(mouseDragged:))]
            fn mouse_dragged(&self, event: &NSEvent) {
                self.moved(event);
            }
            #[unsafe(method(rightMouseDragged:))]
            fn right_mouse_dragged(&self, event: &NSEvent) {
                self.moved(event);
            }
            #[unsafe(method(otherMouseDragged:))]
            fn other_mouse_dragged(&self, event: &NSEvent) {
                self.moved(event);
            }
            #[unsafe(method(mouseExited:))]
            fn mouse_exited(&self, event: &NSEvent) {
                self.exited(event);
            }
            #[unsafe(method(scrollWheel:))]
            fn scroll_wheel(&self, event: &NSEvent) {
                self.wheel(event);
            }
        }
    );
    fn other_button(event: &NSEvent) -> MouseButton {
        match event.buttonNumber() {
            2 => MouseButton::Middle,
            3 => MouseButton::Back,
            4 => MouseButton::Forward,
            n => MouseButton::Other(n as u16),
        }
    }
    impl PopupView {
        fn send(&self, input: PopupInput) {
            let send = self.ivars().send.borrow().clone();
            if let Some(send) = send {
                send(input);
            }
        }
        /// `event`'s point in the parent window's coordinates.
        fn point(&self, event: &NSEvent) -> (f32, f32) {
            let point = self.convertPoint_fromView(event.locationInWindow(), None);
            let (x, y) = self.ivars().origin.get();
            (x + point.x as f32, y + point.y as f32)
        }
        fn button(&self, event: &NSEvent, button: MouseButton, pressed: bool) {
            let (x, y) = self.point(event);
            self.send(PopupInput::Button {
                x,
                y,
                button,
                pressed,
            });
        }
        fn moved(&self, event: &NSEvent) {
            let (x, y) = self.point(event);
            self.send(PopupInput::Moved { x, y });
        }
        fn exited(&self, event: &NSEvent) {
            // Into the parent window, its own events take over (a leave
            // posted after them would clear what they hover).
            let parent = self.ivars().parent.borrow().clone();
            if let Some(parent) = parent {
                let screen = NSEvent::mouseLocation();
                if let Some(content) = parent.contentView() {
                    let frame = parent
                        .convertRectToScreen(content.convertRect_toView(content.bounds(), None));
                    let inside = screen.x >= frame.origin.x
                        && screen.y >= frame.origin.y
                        && screen.x < frame.origin.x + frame.size.width
                        && screen.y < frame.origin.y + frame.size.height;
                    if inside {
                        return;
                    }
                }
            }
            let _ = event;
            self.send(PopupInput::Left);
        }
        fn wheel(&self, event: &NSEvent) {
            let (x, y) = self.point(event);
            let (dx, dy) = (event.scrollingDeltaX(), event.scrollingDeltaY());
            let scale = self
                .window()
                .map_or(1., |window| window.backingScaleFactor());
            // As winit reports the window's own.
            let delta = if event.hasPreciseScrollingDeltas() {
                let delta: PhysicalPosition<f64> = LogicalPosition::new(dx, dy).to_physical(scale);
                MouseScrollDelta::PixelDelta(delta)
            } else {
                MouseScrollDelta::LineDelta(dx as f32, dy as f32)
            };
            let phase = |phase: NSEventPhase| {
                if phase.contains(NSEventPhase::MayBegin) || phase.contains(NSEventPhase::Began) {
                    Some(TouchPhase::Started)
                } else if phase.contains(NSEventPhase::Ended) {
                    Some(TouchPhase::Ended)
                } else if phase.contains(NSEventPhase::Cancelled) {
                    Some(TouchPhase::Cancelled)
                } else {
                    None
                }
            };
            let phase = phase(event.momentumPhase())
                .or_else(|| phase(event.phase()))
                .unwrap_or(TouchPhase::Moved);
            self.send(PopupInput::Wheel { x, y, delta, phase });
        }
    }

    /// What shows behind a popup whose panel is translucent: AppKit's
    /// material for it, cut to the panel's rounded box, as menus and
    /// popovers are made of.
    #[derive(Clone, Copy, PartialEq)]
    struct Material {
        /// The panel's box, in its window's coordinates.
        rect: Rect,
        radius: f32,
        material: NSVisualEffectMaterial,
    }
    impl Material {
        /// For popup root `root`: its panel (what the root holds) is a box
        /// with a translucent fill.
        fn of(scene: &Scene, semantics: &Semantics, root: NodeId) -> Option<Self> {
            let panel = *scene.children(root).first()?;
            let (NodeKind::Quad(quad) | NodeKind::Panel { quad, .. }) = scene.kind(panel) else {
                return None;
            };
            if quad.fill.3 == 255 {
                return None;
            }
            let menu = semantics
                .get(panel)
                .is_some_and(|node| node.role == Role::Menu);
            Some(Self {
                rect: scene.bounds(panel),
                radius: quad.radius,
                material: if menu {
                    NSVisualEffectMaterial::Menu
                } else {
                    NSVisualEffectMaterial::Popover
                },
            })
        }
    }

    /// A popup window: a borderless panel holding AppKit's material (when
    /// its popup wants it) under a view hosting a renderer's layer.
    struct Panel {
        window: Retained<NSPanel>,
        view: Retained<PopupView>,
        effect: Retained<NSVisualEffectView>,
        material: Option<Material>,
        host: Retained<CALayer>,
        renderer: Option<GpuRenderer>,
        /// The scale its renderer was made for.
        scale: f32,
        /// Its frame in its parent window's coordinates, as last placed.
        rect: Option<Rect>,
        /// Its popup root's layer revision it last drew.
        revision: Option<u64>,
    }
    impl Panel {
        fn new(mtm: MainThreadMarker) -> Self {
            let frame = NSRect::new(NSPoint::new(0., 0.), NSSize::new(1., 1.));
            let window = NSPanel::initWithContentRect_styleMask_backing_defer(
                mtm.alloc::<NSPanel>(),
                frame,
                NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel,
                NSBackingStoreType::Buffered,
                false,
            );
            // SAFETY: plain property setters on a window of ours.
            unsafe { window.setReleasedWhenClosed(false) };
            window.setOpaque(false);
            window.setBackgroundColor(Some(&NSColor::clearColor()));
            window.setHasShadow(true);
            // Over its parent and every other window, as AppKit's menus.
            window.setLevel(NSPopUpMenuWindowLevel);
            // Shown at once, and only while zgui shows it (it closes popups
            // as the app deactivates).
            window.setAnimationBehavior(NSWindowAnimationBehavior::None);
            window.setHidesOnDeactivate(false);
            window.setCollectionBehavior(
                NSWindowCollectionBehavior::Transient
                    | NSWindowCollectionBehavior::IgnoresCycle
                    | NSWindowCollectionBehavior::FullScreenAuxiliary,
            );
            window.setAcceptsMouseMovedEvents(true);
            window.setBecomesKeyOnlyIfNeeded(true);
            let host = CALayer::new();
            host.setHidden(true);
            let content = NSView::initWithFrame(mtm.alloc::<NSView>(), frame);
            content.setWantsLayer(true);
            let effect =
                NSVisualEffectView::initWithFrame(mtm.alloc::<NSVisualEffectView>(), frame);
            effect.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
            effect.setState(NSVisualEffectState::Active);
            effect.setWantsLayer(true);
            effect.setHidden(true);
            if let Some(layer) = effect.layer() {
                // SAFETY: CALayer's own property, a string constant.
                unsafe {
                    let continuous = NSString::from_str("continuous");
                    let _: () = msg_send![&*layer, setCornerCurve: &*continuous];
                }
                layer.setMasksToBounds(true);
            }
            content.addSubview(&effect);
            let view: Retained<PopupView> = {
                let view = mtm.alloc::<PopupView>().set_ivars(Target::default());
                // SAFETY: NSView's designated initializer.
                unsafe { msg_send![super(view), initWithFrame: frame] }
            };
            // Layer-hosting: our layer, set before it wants one.
            view.setLayer(Some(&host));
            view.setWantsLayer(true);
            // SAFETY: a tracking area owned by the view it reports to.
            let area = unsafe {
                NSTrackingArea::initWithRect_options_owner_userInfo(
                    mtm.alloc::<NSTrackingArea>(),
                    NSRect::ZERO,
                    NSTrackingAreaOptions::MouseMoved
                        | NSTrackingAreaOptions::MouseEnteredAndExited
                        | NSTrackingAreaOptions::ActiveAlways
                        | NSTrackingAreaOptions::InVisibleRect,
                    Some(&view as &AnyObject),
                    None,
                )
            };
            view.addTrackingArea(&area);
            view.setAutoresizingMask(
                NSAutoresizingMaskOptions::ViewWidthSizable
                    | NSAutoresizingMaskOptions::ViewHeightSizable,
            );
            content.addSubview(&view);
            window.setContentView(Some(&content));
            Self {
                window,
                view,
                effect,
                material: None,
                host,
                renderer: None,
                scale: 0.,
                rect: None,
                revision: None,
            }
        }
        /// Its renderer, made for `scale` (again for a new one).
        fn renderer(
            &mut self,
            context: &GpuContext,
            scale: f32,
        ) -> Result<&mut GpuRenderer, GpuError> {
            if self.scale != scale {
                self.renderer = None;
            }
            if self.renderer.is_none() {
                self.scale = scale;
                self.host.setContentsScale(scale as f64);
                self.renderer = Some(GpuRenderer::for_overlay_layer(
                    self.host.clone(),
                    context,
                    scale,
                )?);
            }
            Ok(self.renderer.as_mut().expect("made above"))
        }
        /// Show `material` behind its frame at `rect`.
        fn back(&mut self, material: Option<Material>, rect: Rect) {
            if self.material == material && self.rect == Some(rect) {
                return;
            }
            self.material = material;
            match material {
                None => self.effect.setHidden(true),
                Some(material) => {
                    self.effect.setMaterial(material.material);
                    self.effect.setFrame(NSRect::new(
                        NSPoint::new(
                            (material.rect.x - rect.x) as f64,
                            // The container runs bottom-up.
                            (rect.y + rect.height - material.rect.y - material.rect.height) as f64,
                        ),
                        NSSize::new(material.rect.width as f64, material.rect.height as f64),
                    ));
                    if let Some(layer) = self.effect.layer() {
                        layer.setCornerRadius(material.radius as f64);
                    }
                    self.effect.setHidden(false);
                }
            }
        }
        /// Place it at `rect` (its parent's coordinates) over `parent`.
        fn place(&mut self, parent: &NSWindow, content: &NSView, rect: Rect) {
            if self.rect == Some(rect) {
                return;
            }
            self.rect = Some(rect);
            let local = NSRect::new(
                NSPoint::new(rect.x as f64, rect.y as f64),
                NSSize::new(rect.width as f64, rect.height as f64),
            );
            let screen = parent.convertRectToScreen(content.convertRect_toView(local, None));
            self.window.setFrame_display(screen, false);
            self.view.ivars().origin.set((rect.x, rect.y));
            // The presenting layers fill the host exactly (see the overlay
            // plane's `show_overlay`).
            let bounds = self.view.bounds();
            self.host.setFrame(bounds);
            if let Some(layers) = unsafe { self.host.sublayers() } {
                for layer in layers.iter() {
                    layer.setFrame(bounds);
                }
            }
        }
        fn hide(&mut self) {
            let parent = self.view.ivars().parent.borrow_mut().take();
            if let Some(parent) = parent {
                parent.removeChildWindow(&self.window);
            }
            self.window.orderOut(None);
            *self.view.ivars().send.borrow_mut() = None;
            // Shown again with its next popup's first frame, never this one's.
            transaction(|| self.host.setHidden(true));
            self.effect.setHidden(true);
            self.material = None;
            self.rect = None;
            self.revision = None;
            self.window.setAlphaValue(1.);
            self.window.setIgnoresMouseEvents(false);
            if let Some(renderer) = &mut self.renderer {
                renderer.trim();
            }
        }
    }
    fn transaction(change: impl FnOnce()) {
        CATransaction::begin();
        CATransaction::setDisableActions(true);
        change();
        CATransaction::commit();
    }

    thread_local! {
        /// Hidden panels, ready to show.
        static POOL_PANELS: RefCell<Vec<Panel>> = const { RefCell::new(Vec::new()) };
        /// Windows showing popups, by window number, and their hosts.
        static PARENTS: RefCell<Vec<(isize, Sink)>> =
            const { RefCell::new(Vec::new()) };
    }
    /// Close a window's popups as it is pressed outside its content (its
    /// title bar), as AppKit closes menus and transient popovers; presses in
    /// its content reach zgui, whose popups close on them. Once per process.
    fn watch_presses() {
        use objc2_app_kit::NSEventMask;
        thread_local! {
            static WATCHING: Cell<bool> = const { Cell::new(false) };
        }
        if WATCHING.replace(true) {
            return;
        }
        let block = block2::RcBlock::new(|event: std::ptr::NonNull<NSEvent>| -> *mut NSEvent {
            // SAFETY: AppKit passes a live event.
            let event_ref = unsafe { event.as_ref() };
            let Some(mtm) = MainThreadMarker::new() else {
                return event.as_ptr();
            };
            let Some(window) = event_ref.window(mtm) else {
                return event.as_ptr();
            };
            let send = PARENTS.with(|parents| {
                parents
                    .borrow()
                    .iter()
                    .find(|(number, _)| *number == window.windowNumber())
                    .map(|(_, send)| send.clone())
            });
            if let (Some(send), Some(content)) = (send, window.contentView()) {
                let point = content.convertPoint_fromView(event_ref.locationInWindow(), None);
                if !content.mouse_inRect(point, content.bounds()) {
                    send(PopupInput::Dismiss);
                }
            }
            event.as_ptr()
        });
        let mask =
            NSEventMask::LeftMouseDown | NSEventMask::RightMouseDown | NSEventMask::OtherMouseDown;
        // SAFETY: the handler returns the event it was given; the monitor
        // lives as long as the app.
        let monitor =
            unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(mask, &block) };
        std::mem::forget(monitor);
    }
    /// Close every window's popups as the app deactivates or hides, as
    /// AppKit's menus close (a window losing focus closes its own too, but an
    /// app at the back never had one). Once per process.
    fn watch_app() {
        use objc2_app_kit::{
            NSApplicationDidHideNotification, NSApplicationWillResignActiveNotification,
        };
        use objc2_foundation::{NSNotification, NSNotificationCenter};
        thread_local! {
            static WATCHING: Cell<bool> = const { Cell::new(false) };
        }
        if WATCHING.replace(true) {
            return;
        }
        let block = block2::RcBlock::new(|_: std::ptr::NonNull<NSNotification>| {
            let parents: Vec<Sink> = PARENTS.with(|parents| {
                parents
                    .borrow()
                    .iter()
                    .map(|(_, send)| send.clone())
                    .collect()
            });
            for send in parents {
                send(PopupInput::Dismiss);
            }
        });
        let center = NSNotificationCenter::defaultCenter();
        // SAFETY: AppKit's own notification names; with no queue given the
        // block runs where AppKit posts them (the main thread), and the
        // observers live as long as the app.
        unsafe {
            for name in [
                NSApplicationWillResignActiveNotification,
                NSApplicationDidHideNotification,
            ] {
                let observer = center.addObserverForName_object_queue_usingBlock(
                    Some(name),
                    None,
                    None,
                    &block,
                );
                std::mem::forget(observer);
            }
        }
    }
    thread_local! {
        /// Popups fading out, by number, until their animation ends.
        static FADING: RefCell<Vec<(u64, Panel)>> = const { RefCell::new(Vec::new()) };
        static NEXT_FADE: Cell<u64> = const { Cell::new(0) };
    }
    /// How long a closing popup takes to fade out, as AppKit's menus do.
    const FADE_OUT: f64 = 0.15;
    /// Fade `panel` out, then put it back in the pool. It hears no pointer
    /// meanwhile: what is under it does.
    fn fade_out(panel: Panel) {
        let window = panel.window.clone();
        window.setIgnoresMouseEvents(true);
        let id = NEXT_FADE.with(|next| {
            next.set(next.get() + 1);
            next.get()
        });
        FADING.with(|fading| fading.borrow_mut().push((id, panel)));
        let changes = block2::RcBlock::new(move |context: *mut AnyObject| {
            // SAFETY: AppKit's animation context, and a window's animator
            // proxy, which animates its alpha.
            unsafe {
                let _: () = msg_send![context, setDuration: FADE_OUT];
                let animator: Retained<NSWindow> = msg_send![&*window, animator];
                animator.setAlphaValue(0.);
            }
        });
        let done = block2::RcBlock::new(move || {
            let panel = FADING.with(|fading| {
                let mut fading = fading.borrow_mut();
                let index = fading.iter().position(|(other, _)| *other == id)?;
                Some(fading.swap_remove(index).1)
            });
            if let Some(panel) = panel {
                release(panel);
            }
        });
        // SAFETY: the class method taking a changes block and a completion
        // block, both copied by AppKit.
        unsafe {
            let _: () = msg_send![
                objc2::class!(NSAnimationContext),
                runAnimationGroup: &*changes,
                completionHandler: &*done
            ];
        }
    }
    fn acquire(mtm: MainThreadMarker) -> Panel {
        POOL_PANELS
            .with(|pool| pool.borrow_mut().pop())
            .unwrap_or_else(|| Panel::new(mtm))
    }
    fn release(mut panel: Panel) {
        panel.hide();
        POOL_PANELS.with(|pool| {
            let mut pool = pool.borrow_mut();
            if pool.len() < pool_size() {
                pool.push(panel);
            }
        });
    }
    /// Have a panel and its renderer ready for the popup about to open
    /// (the pointer reached something that opens one, `Ui::on_popup_soon`),
    /// if none is. It is ordered in once, transparent and off screen, then
    /// out: the window server's first showing of a window is what costs.
    pub(crate) fn warm(context: &GpuContext, scale: f32) {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        if POOL_PANELS.with(|pool| !pool.borrow().is_empty()) {
            return;
        }
        let mut panel = Panel::new(mtm);
        if panel.renderer(context, scale).is_err() {
            return;
        }
        panel.window.setAlphaValue(0.);
        panel
            .window
            .setFrameOrigin(NSPoint::new(-10_000., -10_000.));
        panel.window.orderFront(None);
        panel.window.orderOut(None);
        panel.window.setAlphaValue(1.);
        POOL_PANELS.with(|pool| pool.borrow_mut().push(panel));
    }

    fn parent_of(window: &winit::window::Window) -> Option<(Retained<NSWindow>, Retained<NSView>)> {
        use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let handle = window.window_handle().ok()?;
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return None;
        };
        // SAFETY: winit's content view, alive while its window is.
        let view = unsafe { Retained::<NSView>::retain(handle.ns_view.as_ptr().cast()) }?;
        Some((view.window()?, view))
    }

    /// Where popups may show: the visible part of each screen (less the
    /// menu bar and Dock), in the window's coordinates. None with
    /// `ZGUI_POPUP_WINDOWS=0`: popups stay inside their window, as elsewhere.
    /// Whether popups show in windows of their own: but with
    /// `ZGUI_POPUP_WINDOWS=0` (inside their window, as elsewhere).
    pub(crate) fn enabled() -> bool {
        thread_local! {
            static ON: bool = std::env::var_os("ZGUI_POPUP_WINDOWS").is_none_or(|v| v != "0");
        }
        ON.with(|on| *on)
    }
    pub(crate) fn popup_screens(window: &winit::window::Window) -> Vec<Rect> {
        if !enabled() {
            return Vec::new();
        }
        let Some((parent, content)) = parent_of(window) else {
            return Vec::new();
        };
        let frame = parent.convertRectToScreen(content.convertRect_toView(content.bounds(), None));
        // Screen space runs bottom-up, the window's top-down.
        let top = frame.origin.y + frame.size.height;
        objc2_app_kit::NSScreen::screens(MainThreadMarker::from(&*parent))
            .iter()
            .map(|screen| {
                let screen = screen.visibleFrame();
                Rect::new(
                    (screen.origin.x - frame.origin.x) as f32,
                    (top - (screen.origin.y + screen.size.height)) as f32,
                    screen.size.width as f32,
                    screen.size.height as f32,
                )
            })
            .collect()
    }
    /// The backing scale of the screen holding `rect`'s centre (the
    /// parent's coordinates): a popup draws at the scale it shows at.
    fn scale_at(parent: &NSWindow, content: &NSView, rect: Rect) -> Option<f32> {
        let local = NSRect::new(
            NSPoint::new(
                (rect.x + rect.width / 2.) as f64,
                (rect.y + rect.height / 2.) as f64,
            ),
            NSSize::new(0., 0.),
        );
        let point = parent
            .convertRectToScreen(content.convertRect_toView(local, None))
            .origin;
        objc2_app_kit::NSScreen::screens(MainThreadMarker::from(parent))
            .iter()
            .find(|screen| {
                let frame = screen.frame();
                (frame.origin.x..frame.origin.x + frame.size.width).contains(&point.x)
                    && (frame.origin.y..frame.origin.y + frame.size.height).contains(&point.y)
            })
            .map(|screen| screen.backingScaleFactor() as f32)
    }

    /// A window's shown popups.
    pub(crate) struct PopupHost {
        shown: Vec<(NodeId, Panel)>,
        send: Sink,
    }
    impl PopupHost {
        pub fn new(send: impl Fn(PopupInput) + 'static) -> Self {
            Self {
                shown: Vec::new(),
                send: Rc::new(send),
            }
        }
        /// Whether any popup shows.
        pub fn showing(&self) -> bool {
            !self.shown.is_empty()
        }
        /// After the scene's flush: show each visible popup root in a panel
        /// over `window`, draw it again where it changed (its layer revision,
        /// `Scene::set_popup`; always when `full`), and hide the rest.
        #[allow(clippy::too_many_arguments)]
        pub fn update(
            &mut self,
            window: &winit::window::Window,
            scene: &Scene,
            semantics: &Semantics,
            context: &GpuContext,
            scale: f32,
            full: bool,
        ) -> Result<(), GpuError> {
            if self.shown.is_empty() && scene.popups().is_empty() {
                return Ok(());
            }
            let Some(mtm) = MainThreadMarker::new() else {
                return Ok(());
            };
            let Some((parent, content)) = parent_of(window) else {
                return Ok(());
            };
            let wanted: Vec<(NodeId, Rect)> = scene
                .popups()
                .iter()
                .filter_map(|root| scene.popup_bounds(*root).map(|bounds| (*root, bounds)))
                .collect();
            let mut index = 0;
            while index < self.shown.len() {
                if wanted.iter().any(|(root, _)| *root == self.shown[index].0) {
                    index += 1;
                    continue;
                }
                let (root, panel) = self.shown.swap_remove(index);
                zgui_gpu::present_log::log(|| format!("popup hide {root:?}"));
                fade_out(panel);
            }
            let number = parent.windowNumber();
            PARENTS.with(|parents| {
                let mut parents = parents.borrow_mut();
                parents.retain(|(window, _)| *window != number);
                if !wanted.is_empty() {
                    parents.push((number, self.send.clone()));
                }
            });
            if !wanted.is_empty() {
                watch_presses();
                watch_app();
            }
            for (root, bounds) in wanted {
                let new = !self.shown.iter().any(|(shown, _)| *shown == root);
                if new {
                    zgui_gpu::present_log::log(|| format!("popup show {root:?} at {bounds:?}"));
                    self.shown.push((root, acquire(mtm)));
                }
                let panel = &mut self
                    .shown
                    .iter_mut()
                    .find(|(shown, _)| *shown == root)
                    .expect("pushed above")
                    .1;
                // Its screen's scale, found as it shows.
                let scale = match new {
                    true => scale_at(&parent, &content, bounds).unwrap_or(scale),
                    false => panel.scale,
                };
                // Popups are small: one that changed is drawn whole.
                let revision = scene.layer_revision(root);
                let changed = new || full || panel.revision != Some(revision);
                panel.revision = Some(revision);
                let renderer = panel.renderer(context, scale)?;
                renderer.set_popup(root, bounds);
                let rect = renderer.plane_rect().unwrap_or(bounds);
                let resized = panel
                    .rect
                    .is_none_or(|old| (old.width, old.height) != (rect.width, rect.height));
                if changed || resized {
                    let renderer = panel.renderer.as_mut().expect("made above");
                    renderer.repaint_all();
                    renderer.render(scene, &[])?;
                }
                let moved = panel.rect != Some(rect);
                let material = Material::of(scene, semantics, root);
                let drawn = panel
                    .renderer
                    .as_ref()
                    .is_some_and(GpuRenderer::frame_pending);
                if !new && !moved && !drawn && material == panel.material {
                    // Nothing new to show: no transaction for the render
                    // server either.
                    continue;
                }
                // The frame and where it shows change together.
                CATransaction::begin();
                CATransaction::setDisableActions(true);
                panel.renderer.as_mut().expect("made above").present()?;
                panel.back(material, rect);
                panel.place(&parent, &content, rect);
                if new {
                    panel.host.setHidden(false);
                }
                CATransaction::commit();
                if new {
                    let ivars = panel.view.ivars();
                    *ivars.send.borrow_mut() = Some(self.send.clone());
                    *ivars.parent.borrow_mut() = Some(parent.clone());
                    // Its first frame reaches the window server before it shows.
                    CATransaction::flush();
                    // SAFETY: both are live windows of this process.
                    unsafe {
                        parent.addChildWindow_ordered(&panel.window, NSWindowOrderingMode::Above)
                    };
                    // A child takes its parent's level; menus' is higher.
                    panel.window.setLevel(NSPopUpMenuWindowLevel);
                    panel.window.orderFront(None);
                }
                if new || moved {
                    // Its rim (macOS 26 menus have no drop shadow, only a
                    // rim) follows what it shows.
                    panel.window.invalidateShadow();
                }
            }
            Ok(())
        }
        /// Hide every popup (the window closes or goes away).
        pub fn hide_all(&mut self, _: &winit::window::Window) {
            for (_, panel) in self.shown.drain(..) {
                release(panel);
            }
        }
    }
    impl Drop for PopupHost {
        fn drop(&mut self) {
            for (_, panel) in self.shown.drain(..) {
                release(panel);
            }
        }
    }
}

#[cfg(target_os = "linux")]
mod x11 {
    use super::PopupInput;
    use std::{
        cell::{Cell, RefCell},
        collections::HashMap,
        rc::Rc,
        sync::Arc,
    };
    use winit::{
        dpi::{PhysicalPosition, PhysicalSize},
        event::WindowEvent,
        event_loop::ActiveEventLoop,
        platform::x11::{WindowAttributesExtX11, WindowType},
        raw_window_handle::{HasDisplayHandle, RawDisplayHandle},
        window::{Window, WindowId},
    };
    use zgui::{
        scene::{NodeId, Rect, Scene},
        semantics::{Role, Semantics},
    };
    use zgui_gpu::{GpuContext, GpuError, GpuRenderer};

    /// Popup windows kept made while hidden: a menu, its submenu and a
    /// tooltip open together without making one.
    const POOL: usize = 3;
    /// Where a popup window is mapped before its first frame and moved to
    /// before it is unmapped: off every screen. An unmapped window's
    /// swapchain never completes a present (Mesa's X11 presentation waits
    /// for the server's notice), so a frame only goes to a mapped window;
    /// and hidden ones stay unmapped, so they are no window of the app's
    /// to anything looking for one.
    const PARKED: PhysicalPosition<i32> = PhysicalPosition::new(-10_000, -10_000);

    /// What hands a popup's input to its parent window's host.
    type Sink = Rc<dyn Fn(PopupInput)>;
    /// Where a popup window's input goes: its popup's origin in its parent
    /// window, the scale, and the parent's host.
    struct Target {
        origin: Cell<(f32, f32)>,
        scale: Cell<f64>,
        cursor: Cell<(f32, f32)>,
        send: Sink,
    }
    struct Popup {
        window: Arc<Window>,
        renderer: Option<GpuRenderer>,
        revision: Option<u64>,
        rect: Option<Rect>,
    }
    thread_local! {
        static HIDDEN: RefCell<Vec<Popup>> = const { RefCell::new(Vec::new()) };
        static TARGETS: RefCell<HashMap<WindowId, Rc<Target>>> = RefCell::new(HashMap::new());
        /// A popup found no window ready: make one where the event loop is.
        static WANTED: Cell<bool> = const { Cell::new(false) };
        static ON: bool = std::env::var_os("ZGUI_POPUP_WINDOWS").is_none_or(|v| v != "0");
    }

    pub(crate) fn enabled() -> bool {
        ON.with(|on| *on)
    }
    fn x11(window: &Window) -> bool {
        enabled()
            && window.display_handle().is_ok_and(|handle| {
                matches!(
                    handle.as_raw(),
                    RawDisplayHandle::Xlib(_) | RawDisplayHandle::Xcb(_)
                )
            })
    }
    /// The monitors popups may show on, in `window`'s logical coordinates
    /// (X11; none on Wayland, where popups stay inside their window).
    pub(crate) fn popup_screens(window: &Window) -> Vec<Rect> {
        if !x11(window) {
            return Vec::new();
        }
        let Ok(origin) = window.inner_position() else {
            return Vec::new();
        };
        let scale = window.scale_factor();
        // Each monitor less the panels on it (the current desktop's work
        // area, `_NET_WORKAREA`), as menus keep off a panel.
        let work = crate::x11_work_area::current();
        window
            .available_monitors()
            .filter_map(|monitor| {
                let (at, size) = (monitor.position(), monitor.size());
                let screen = (at.x, at.y, size.width as i32, size.height as i32);
                let (x, y, width, height) = match work {
                    Some(work) => crate::x11_work_area::intersect(screen, work)?,
                    None => screen,
                };
                Some(Rect::new(
                    ((x - origin.x) as f64 / scale) as f32,
                    ((y - origin.y) as f64 / scale) as f32,
                    (width as f64 / scale) as f32,
                    (height as f64 / scale) as f32,
                ))
            })
            .collect()
    }
    /// An X11 window's id.
    fn xid(window: &Window) -> Option<u32> {
        use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
        match window.window_handle().ok()?.as_raw() {
            RawWindowHandle::Xlib(handle) => Some(handle.window as u32),
            RawWindowHandle::Xcb(handle) => Some(handle.window.get()),
            _ => None,
        }
    }
    thread_local! {
        /// A connection of zgui's own, for what winit does not ask.
        static CONNECTION: Option<x11rb::rust_connection::RustConnection> =
            x11rb::connect(None).ok().map(|(connection, _)| connection);
    }
    /// Mark `popup` its parent's (`WM_TRANSIENT_FOR`), as menus are: what
    /// compositors and accessibility read it as part of.
    fn transient_for(popup: &Window, parent: &Window) {
        use x11rb::{
            connection::Connection as _,
            protocol::xproto::{AtomEnum, PropMode},
            wrapper::ConnectionExt as _,
        };
        let (Some(popup), Some(parent)) = (xid(popup), xid(parent)) else {
            return;
        };
        CONNECTION.with(|connection| {
            if let Some(connection) = connection {
                let _ = connection.change_property32(
                    PropMode::REPLACE,
                    popup,
                    AtomEnum::WM_TRANSIENT_FOR,
                    AtomEnum::WINDOW,
                    &[parent],
                );
                let _ = connection.flush();
            }
        });
    }
    /// winit's X display: XI2 requests go on its connection, as winit hears
    /// XI2 events alone.
    fn display(window: &Window) -> Option<*mut x11_dl::xlib::Display> {
        use winit::raw_window_handle::RawDisplayHandle;
        match window.display_handle().ok()?.as_raw() {
            RawDisplayHandle::Xlib(handle) => Some(handle.display?.as_ptr().cast()),
            _ => None,
        }
    }
    thread_local! {
        static XI: Option<(x11_dl::xinput2::XInput2, x11_dl::xlib::Xlib)> = (|| {
            Some((
                x11_dl::xinput2::XInput2::open().ok()?,
                x11_dl::xlib::Xlib::open().ok()?,
            ))
        })();
    }
    /// Hold the pointer for `parent` while a menu or popover shows, as GTK's
    /// menus do: presses in the app's windows arrive as ever (owner
    /// events), and one anywhere else arrives at `parent`, outside it, which
    /// closes its popups. The display held, to let go with `release_grab`.
    fn grab(parent: &Window) -> Option<usize> {
        use x11_dl::{xinput2, xlib};
        let (window, display) = (xid(parent)?, display(parent)?);
        XI.with(|xi| {
            let (xi, x) = xi.as_ref()?;
            // SAFETY: winit's open display, on its event loop's thread; the
            // mask outlives the call.
            unsafe {
                let mut pointer = 0;
                if (xi.XIGetClientPointer)(display, 0, &mut pointer) == 0 {
                    return None;
                }
                let mut bits = [0u8; 4];
                for event in [
                    xinput2::XI_ButtonPress,
                    xinput2::XI_ButtonRelease,
                    xinput2::XI_Motion,
                ] {
                    bits[event as usize >> 3] |= 1 << (event & 7);
                }
                let mut mask = xinput2::XIEventMask {
                    deviceid: pointer,
                    mask_len: bits.len() as i32,
                    mask: bits.as_mut_ptr(),
                };
                let status = (xi.XIGrabDevice)(
                    display,
                    pointer,
                    window as _,
                    xlib::CurrentTime,
                    0,
                    xlib::GrabModeAsync,
                    xlib::GrabModeAsync,
                    xlib::True,
                    &mut mask,
                );
                (x.XFlush)(display);
                (status == xlib::GrabSuccess).then_some(display as usize)
            }
        })
    }
    /// Let go of the pointer `grab` held on `display`.
    fn release_grab(display: usize) {
        let display = display as *mut x11_dl::xlib::Display;
        XI.with(|xi| {
            let Some((xi, x)) = xi.as_ref() else {
                return;
            };
            // SAFETY: winit's display, still open: the event loop outlives
            // the windows whose popups grabbed.
            unsafe {
                let mut pointer = 0;
                if (xi.XIGetClientPointer)(display, 0, &mut pointer) != 0 {
                    (xi.XIUngrabDevice)(display, pointer, x11_dl::xlib::CurrentTime);
                    (x.XFlush)(display);
                }
            }
        });
    }
    /// macOS readies a panel here; X11's windows are made by `make`.
    pub(crate) fn warm(_: &GpuContext, _: f32) {}
    /// Whether a popup found no window ready (`make` one).
    pub(crate) fn wanted() -> bool {
        WANTED.with(|wanted| wanted.replace(false))
    }
    /// Have a hidden popup window ready, if none is.
    pub(crate) fn make(event_loop: &ActiveEventLoop) {
        let x11 = event_loop.display_handle().is_ok_and(|handle| {
            matches!(
                handle.as_raw(),
                RawDisplayHandle::Xlib(_) | RawDisplayHandle::Xcb(_)
            )
        });
        if !enabled() || !x11 {
            return;
        }
        HIDDEN.with(|hidden| {
            let mut hidden = hidden.borrow_mut();
            if hidden.is_empty() {
                let attributes = Window::default_attributes()
                    .with_visible(false)
                    // Its parent keeps the keyboard (and its popups).
                    .with_active(false)
                    .with_position(PARKED)
                    .with_decorations(false)
                    .with_transparent(true)
                    .with_inner_size(PhysicalSize::new(1, 1))
                    .with_override_redirect(true)
                    .with_x11_window_type(vec![WindowType::PopupMenu]);
                let Ok(window) = event_loop.create_window(attributes) else {
                    return;
                };
                hidden.push(Popup {
                    window: Arc::new(window),
                    renderer: None,
                    revision: None,
                    rect: None,
                });
            }
        });
    }
    /// Where the pointer is in `window` (physical), as it comes back from
    /// a popup window: winit tells the parent only that the pointer
    /// entered, and sends no move to a point it last saw it at, so the
    /// parent would act where the popup last had it.
    pub(crate) fn pointer(window: &Window) -> Option<PhysicalPosition<f64>> {
        use x11rb::protocol::xproto::ConnectionExt as _;
        if !x11(window) {
            return None;
        }
        let id = xid(window)?;
        CONNECTION.with(|connection| {
            let reply = connection.as_ref()?.query_pointer(id).ok()?.reply().ok()?;
            reply
                .same_screen
                .then(|| PhysicalPosition::new(reply.win_x as f64, reply.win_y as f64))
        })
    }
    /// A popup window's event, as its parent's host hears it; whether
    /// `window` was a popup's.
    pub(crate) fn route(window: WindowId, event: &WindowEvent) -> bool {
        let Some(target) = TARGETS.with(|targets| targets.borrow().get(&window).cloned()) else {
            return HIDDEN.with(|hidden| hidden.borrow().iter().any(|p| p.window.id() == window));
        };
        let (x, y) = target.origin.get();
        let scale = target.scale.get();
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                let at = (
                    x + (position.x / scale) as f32,
                    y + (position.y / scale) as f32,
                );
                target.cursor.set(at);
                (target.send)(PopupInput::Moved { x: at.0, y: at.1 });
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let (x, y) = target.cursor.get();
                (target.send)(PopupInput::Button {
                    x,
                    y,
                    button: *button,
                    pressed: state.is_pressed(),
                });
            }
            WindowEvent::MouseWheel { delta, phase, .. } => {
                let (x, y) = target.cursor.get();
                (target.send)(PopupInput::Wheel {
                    x,
                    y,
                    delta: *delta,
                    phase: *phase,
                });
            }
            WindowEvent::CursorLeft { .. } => (target.send)(PopupInput::Left),
            _ => {}
        }
        true
    }

    /// A window's shown popups.
    pub(crate) struct PopupHost {
        shown: Vec<(NodeId, Popup)>,
        send: Sink,
        /// The display whose pointer a menu or popover holds (`grab`).
        grabbed: Option<usize>,
    }
    impl PopupHost {
        pub fn new(send: impl Fn(PopupInput) + 'static) -> Self {
            Self {
                shown: Vec::new(),
                send: Rc::new(send),
                grabbed: None,
            }
        }
        /// Hold the pointer for `parent`, or let go of it.
        fn hold(&mut self, parent: &Window, on: bool) {
            if on && self.grabbed.is_none() {
                self.grabbed = grab(parent);
            } else if !on && let Some(display) = self.grabbed.take() {
                release_grab(display);
            }
        }
        /// Whether any popup shows.
        pub fn showing(&self) -> bool {
            !self.shown.is_empty()
        }
        /// After the scene's flush: show each visible popup root in a window
        /// over `parent`, draw it again where it changed (always when
        /// `full`), and hide the rest.
        #[allow(clippy::too_many_arguments)]
        pub fn update(
            &mut self,
            parent: &Window,
            scene: &Scene,
            semantics: &Semantics,
            context: &GpuContext,
            scale: f32,
            full: bool,
        ) -> Result<(), GpuError> {
            if (self.shown.is_empty() && scene.popups().is_empty()) || !x11(parent) {
                return Ok(());
            }
            let wanted: Vec<(NodeId, Rect)> = scene
                .popups()
                .iter()
                .filter_map(|root| scene.popup_bounds(*root).map(|bounds| (*root, bounds)))
                .collect();
            let mut index = 0;
            while index < self.shown.len() {
                if wanted.iter().any(|(root, _)| *root == self.shown[index].0) {
                    index += 1;
                    continue;
                }
                let (root, popup) = self.shown.swap_remove(index);
                zgui_gpu::present_log::log(|| format!("popup hide {root:?}"));
                release(popup);
            }
            let Ok(origin) = parent.inner_position() else {
                return Ok(());
            };
            for (root, bounds) in wanted {
                let new = !self.shown.iter().any(|(shown, _)| *shown == root);
                if new {
                    zgui_gpu::present_log::log(|| format!("popup show {root:?} at {bounds:?}"));
                    let Some(popup) = HIDDEN.with(|hidden| hidden.borrow_mut().pop()) else {
                        // Made where the event loop is; drawn next frame.
                        WANTED.with(|wanted| wanted.set(true));
                        parent.request_redraw();
                        continue;
                    };
                    // Mapped off screen, to take its first frame.
                    popup.window.set_visible(true);
                    transient_for(&popup.window, parent);
                    self.shown.push((root, popup));
                }
                let popup = &mut self
                    .shown
                    .iter_mut()
                    .find(|(shown, _)| *shown == root)
                    .expect("pushed above")
                    .1;
                if popup.renderer.is_none() {
                    popup.renderer = Some(GpuRenderer::for_window_with_context(
                        popup.window.clone(),
                        context,
                    )?);
                }
                let renderer = popup.renderer.as_mut().expect("made above");
                renderer.set_popup(root, bounds);
                let rect = renderer.plane_rect().unwrap_or(bounds);
                let moved = popup.rect != Some(rect);
                if moved {
                    // The window to the popup's box, in whole device pixels,
                    // before its surface draws at that size.
                    let size = PhysicalSize::new(
                        (rect.width * scale).round() as u32,
                        (rect.height * scale).round() as u32,
                    );
                    let _ = popup.window.request_inner_size(size);
                }
                // Popups are small: one that changed is drawn whole.
                let revision = scene.layer_revision(root);
                if new || full || moved || popup.revision != Some(revision) {
                    popup.revision = Some(revision);
                    renderer.repaint_all();
                    renderer.render(scene, &[])?;
                    renderer.present()?;
                }
                // Onto the screen once its frame is there.
                if moved {
                    popup.window.set_outer_position(PhysicalPosition::new(
                        origin.x + (rect.x * scale).round() as i32,
                        origin.y + (rect.y * scale).round() as i32,
                    ));
                }
                popup.rect = Some(rect);
                TARGETS.with(|targets| {
                    let mut targets = targets.borrow_mut();
                    let target = targets.entry(popup.window.id()).or_insert_with(|| {
                        Rc::new(Target {
                            origin: Cell::new((rect.x, rect.y)),
                            scale: Cell::new(scale as f64),
                            cursor: Cell::new((rect.x, rect.y)),
                            send: self.send.clone(),
                        })
                    });
                    target.origin.set((rect.x, rect.y));
                    target.scale.set(scale as f64);
                });
            }
            // A menu or popover holds the pointer; a tooltip does not.
            let modal = self.shown.iter().any(|(root, _)| {
                semantics
                    .get(*root)
                    .is_some_and(|node| matches!(node.role, Role::Menu | Role::Dialog))
            });
            self.hold(parent, modal);
            Ok(())
        }
        /// Hide every popup (the window closes or goes away).
        pub fn hide_all(&mut self, parent: &Window) {
            for (_, popup) in self.shown.drain(..) {
                release(popup);
            }
            self.hold(parent, false);
        }
    }
    impl Drop for PopupHost {
        fn drop(&mut self) {
            for (_, popup) in self.shown.drain(..) {
                release(popup);
            }
            if let Some(display) = self.grabbed.take() {
                release_grab(display);
            }
        }
    }
    /// Hide `popup` (parked off screen, unmapped), and keep it made if few
    /// are.
    fn release(mut popup: Popup) {
        popup.window.set_outer_position(PARKED);
        popup.window.set_visible(false);
        popup.revision = None;
        popup.rect = None;
        TARGETS.with(|targets| targets.borrow_mut().remove(&popup.window.id()));
        HIDDEN.with(|hidden| {
            let mut hidden = hidden.borrow_mut();
            if hidden.len() < POOL {
                hidden.push(popup);
            }
        });
    }
}
