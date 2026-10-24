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
//! Windows does the same over owned, never-activated popup windows drawn
//! through DirectComposition (`popup_window/win32.rs`).
//!
//! Elsewhere, popups stay inside their window (the host gives no screens to
//! show them on, `Ui::set_popup_screens`).
//!
//! X11 (`x11`): override-redirect ARGB windows typed as popup menus
//! (`_NET_WM_WINDOW_TYPE_POPUP_MENU`), placed by the app on the monitors
//! winit reports, dismissed with the parent's focus. winit makes them, so
//! they are made where the event loop is (`make`, as the pointer nears a
//! trigger, or the frame after a popup found none ready).
//!
//! Wayland (`wayland`): `xdg_popup`s of the window's `xdg_surface` (the
//! vendored winit's), which the compositor places and keeps on its outputs.
use winit::event::{MouseButton, MouseScrollDelta, TouchPhase};

/// Pointer input over a popup window, in its parent window's coordinates.
#[derive(Clone, Copy, Debug)]
#[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
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
    /// The platform cut popup `root` to `size` (points) where the screen
    /// had no room for it, or it closed (`None`): `Ui::set_popup_limit`.
    /// (Wayland's; elsewhere the scene knows the screens.)
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Limit {
        root: zgui::scene::NodeId,
        size: Option<(f32, f32)>,
    },
    /// The platform showed popup `root` on its trigger's other side, across
    /// and down, or it closed (`(false, false)`): `Ui::set_popup_flipped`.
    /// (Wayland's, as `Limit`.)
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Flip {
        root: zgui::scene::NodeId,
        flipped: (bool, bool),
    },
    /// The screens changed (a monitor, the taskbar), or popups can have no
    /// windows of their own after all: `popup_screens` again. (Windows'; AppKit
    /// and X11 popups follow screens as their windows move.)
    #[cfg_attr(not(windows), allow(dead_code))]
    Screens,
}

#[cfg(target_os = "macos")]
pub(crate) use macos::{
    PopupHost, content_origin, enabled, popup_screens, server_move, warm, window_at, window_number,
};

#[cfg(windows)]
mod win32;
#[cfg(windows)]
pub(crate) use win32::{
    PopupHost, content_origin, enabled, popup_screens, warm, window_at, window_number,
};

#[cfg(target_os = "linux")]
pub(crate) use x11::{
    PopupHost, content_origin, enabled, global_pointer, make, move_window, pointer, popup_screens,
    route, settle_window, wanted, warm, window_at, window_below, window_number,
};

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
pub(crate) use other::{
    PopupHost, content_origin, enabled, popup_screens, warm, window_at, window_number,
};

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
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
            _: Option<&zgui::input::DragState>,
            _: Option<zgui::input::DragEnded>,
        ) -> Result<(), zgui_gpu::GpuError> {
            Ok(())
        }
        pub fn follow(&mut self, _: &winit::window::Window, _: &zgui::input::DragState) {}
        pub fn preview_image(
            &self,
            _: &zgui::input::DragState,
        ) -> Option<crate::native_drag::DragImage> {
            None
        }
        pub fn show_preview(
            &mut self,
            _: &winit::window::Window,
            _: &zgui::input::DragState,
            _: Option<(f32, f32)>,
        ) {
        }
        pub fn hide_all(&mut self, _: &winit::window::Window) {}
    }
    pub(crate) fn popup_screens(_: &winit::window::Window) -> Vec<Rect> {
        Vec::new()
    }
    pub(crate) fn enabled() -> bool {
        false
    }
    // todo: drags between windows need each window's place on screen and
    // the window under a point (Windows: `GetWindowRect`, `WindowFromPoint`;
    // X11: `XTranslateCoordinates`, the stacking order). Without them a
    // drag stays in its window.
    pub(crate) fn content_origin(_: &winit::window::Window) -> Option<(f64, f64)> {
        None
    }
    pub(crate) fn window_number(_: &winit::window::Window) -> Option<isize> {
        None
    }
    pub(crate) fn window_at(_: f64, _: f64) -> Option<isize> {
        None
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
        input::{DragEnded, DragState},
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
    // SAFETY: an NSView subclass, main-thread only as views are, with nothing
    // to clean up beyond its ivars; each method has NSView's or NSResponder's
    // signature (`-(BOOL)isFlipped`, `-(BOOL)acceptsFirstMouse:(nullable
    // NSEvent *)`, and `-(void)…:(NSEvent *)` for the mouse events).
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
    /// material for it, cut to the panel's rounded box (and its arrow, a
    /// popover's), as menus and popovers are made of.
    #[derive(Clone, Copy, PartialEq)]
    struct Material {
        /// The panel's box, in its window's coordinates.
        rect: Rect,
        radius: f32,
        material: NSVisualEffectMaterial,
        /// A popover's arrow (`View::arrow`): its triangle past the panel's
        /// edge, in its window's coordinates.
        arrow: Option<[(f32, f32); 3]>,
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
            let rect = scene.bounds(panel);
            // The arrow's box beside the panel: its triangle points away.
            let arrow = scene
                .children(root)
                .get(1)
                .filter(|arrow| scene.style(**arrow).clip)
                .map(|arrow| {
                    let b = scene.bounds(*arrow);
                    let (right, bottom) = (b.x + b.width, b.y + b.height);
                    let (centre_x, centre_y) = (b.x + b.width / 2., b.y + b.height / 2.);
                    if bottom <= rect.y + 0.5 {
                        [(b.x, bottom), (right, bottom), (centre_x, b.y)]
                    } else if b.y >= rect.y + rect.height - 0.5 {
                        [(b.x, b.y), (right, b.y), (centre_x, bottom)]
                    } else if right <= rect.x + 0.5 {
                        [(right, b.y), (right, bottom), (b.x, centre_y)]
                    } else {
                        [(b.x, b.y), (b.x, bottom), (right, centre_y)]
                    }
                });
            Some(Self {
                rect,
                radius: quad.radius,
                material: if menu {
                    NSVisualEffectMaterial::Menu
                } else {
                    NSVisualEffectMaterial::Popover
                },
                arrow,
            })
        }
        /// Moved by `(dx, dy)`.
        fn moved(mut self, dx: f32, dy: f32) -> Self {
            self.rect.x += dx;
            self.rect.y += dy;
            if let Some(arrow) = &mut self.arrow {
                for point in arrow {
                    point.0 += dx;
                    point.1 += dy;
                }
            }
            self
        }
        /// Its box: the panel's, and the arrow's.
        fn bounds(&self) -> Rect {
            let mut bounds = self.rect;
            for (x, y) in self.arrow.into_iter().flatten() {
                let (right, bottom) = (bounds.x + bounds.width, bounds.y + bounds.height);
                bounds.x = bounds.x.min(x);
                bounds.y = bounds.y.min(y);
                bounds.width = right.max(x) - bounds.x;
                bounds.height = bottom.max(y) - bounds.y;
            }
            bounds
        }
    }
    /// A mask of `material`'s shape (its rounded box and arrow), the size of
    /// its bounds, top-down.
    fn arrow_mask(material: &Material) -> Retained<objc2_app_kit::NSImage> {
        use objc2_app_kit::{NSBezierPath, NSImage};
        let bounds = material.bounds();
        let (rect, radius, arrow) = (material.rect, material.radius as f64, material.arrow);
        let draw = block2::RcBlock::new(move |_: NSRect| -> objc2::runtime::Bool {
            NSColor::blackColor().set();
            let local = |x: f32, y: f32| NSPoint::new((x - bounds.x) as f64, (y - bounds.y) as f64);
            NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(
                NSRect::new(
                    local(rect.x, rect.y),
                    NSSize::new(rect.width as f64, rect.height as f64),
                ),
                radius,
                radius,
            )
            .fill();
            if let Some([a, b, c]) = arrow {
                let path = NSBezierPath::bezierPath();
                path.moveToPoint(local(a.0, a.1));
                path.lineToPoint(local(b.0, b.1));
                path.lineToPoint(local(c.0, c.1));
                path.closePath();
                path.fill();
            }
            objc2::runtime::Bool::YES
        });
        NSImage::imageWithSize_flipped_drawingHandler(
            NSSize::new(bounds.width as f64, bounds.height as f64),
            true,
            &draw,
        )
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
        /// A drag preview's: from where its preview goes to its frame.
        preview: Option<(f32, f32)>,
        /// Moved by the window server alone (`follow`), to `rect` over its
        /// parent: AppKit's frame is stale until set.
        moved: Option<(Retained<NSWindow>, Retained<NSView>, Rect)>,
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
            // SAFETY: turns release-on-close off, as `Retained` owning the
            // window requires (closing would otherwise release it twice).
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
                // SAFETY: `setCornerCurve:` takes a CALayerCornerCurve, an
                // NSString ("continuous" is `kCACornerCurveContinuous`'s).
                unsafe {
                    let continuous = NSString::from_str("continuous");
                    let _: () = msg_send![&*layer, setCornerCurve: &*continuous];
                }
                layer.setMasksToBounds(true);
            }
            content.addSubview(&effect);
            let view: Retained<PopupView> = {
                let view = mtm.alloc::<PopupView>().set_ivars(Target::default());
                // SAFETY: NSView's designated initializer, on a fresh
                // allocation with its ivars set.
                unsafe { msg_send![super(view), initWithFrame: frame] }
            };
            // Layer-hosting: our layer, set before it wants one.
            view.setLayer(Some(&host));
            view.setWantsLayer(true);
            // SAFETY: the area keeps its owner unretained: the owner is the
            // view the area is added to, which keeps the area, so the owner
            // outlives the area's use; no user info.
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
                preview: None,
                moved: None,
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
                    let bounds = material.bounds();
                    self.effect.setFrame(NSRect::new(
                        NSPoint::new(
                            (bounds.x - rect.x) as f64,
                            // The container runs bottom-up.
                            (rect.y + rect.height - bounds.y - bounds.height) as f64,
                        ),
                        NSSize::new(bounds.width as f64, bounds.height as f64),
                    ));
                    // A popover's arrow: the shape is a mask; else the
                    // layer's rounded corners cut it.
                    match material.arrow {
                        Some(_) => {
                            if let Some(layer) = self.effect.layer() {
                                layer.setCornerRadius(0.);
                            }
                            self.effect.setMaskImage(Some(&arrow_mask(&material)));
                        }
                        None => {
                            self.effect.setMaskImage(None);
                            if let Some(layer) = self.effect.layer() {
                                layer.setCornerRadius(material.radius as f64);
                            }
                        }
                    }
                    self.effect.setHidden(false);
                }
            }
        }
        /// Place it at `rect` (its parent's coordinates) over `parent`.
        fn place(&mut self, parent: &NSWindow, content: &NSView, rect: Rect) {
            self.sync();
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
            // SAFETY: the layer's live sublayer list, which setting frames
            // does not change while it is walked.
            if let Some(layers) = unsafe { self.host.sublayers() } {
                for layer in layers.iter() {
                    layer.setFrame(bounds);
                }
            }
        }
        /// AppKit learns where the window server has moved it (`follow`).
        fn sync(&mut self) {
            if let Some((parent, content, rect)) = self.moved.take() {
                self.rect = None;
                self.place(&parent, &content, rect);
            }
        }
        fn hide(&mut self) {
            self.sync();
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
            if self.preview.take().is_some() {
                let number = self.window.windowNumber();
                PREVIEWS.with(|previews| previews.borrow_mut().retain(|other| *other != number));
            }
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
        /// Panels showing drag previews, by window number: the pointer is
        /// over what is under them (`window_at`).
        static PREVIEWS: RefCell<Vec<isize>> = const { RefCell::new(Vec::new()) };
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
            // SAFETY: AppKit passes a live event for the handler's duration.
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
            // SAFETY: AppKit's NSAnimationContext (`setDuration:` takes an
            // NSTimeInterval, f64), and the window's animator proxy, which
            // forwards NSWindow's messages (its alpha animates); `animator`
            // returns it autoreleased, which `Retained` retains.
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
        // SAFETY: `+runAnimationGroup:completionHandler:` takes a
        // `void (^)(NSAnimationContext *)` and a `void (^)(void)` block,
        // both copied by AppKit; it calls them on the main thread.
        unsafe {
            let _: () = msg_send![
                objc2::class!(NSAnimationContext),
                runAnimationGroup: &*changes,
                completionHandler: &*done
            ];
        }
    }
    /// Slide a drag preview's `panel` to `to` (its parent's coordinates),
    /// with the duration and ease of AppKit's drag images sliding back, then
    /// put it back in the pool.
    fn slide_back(panel: Panel, parent: &NSWindow, content: &NSView, to: Rect) {
        let local = NSRect::new(
            NSPoint::new(to.x as f64, to.y as f64),
            NSSize::new(to.width as f64, to.height as f64),
        );
        let target = parent.convertRectToScreen(content.convertRect_toView(local, None));
        let window = panel.window.clone();
        let id = NEXT_FADE.with(|next| {
            next.set(next.get() + 1);
            next.get()
        });
        FADING.with(|fading| fading.borrow_mut().push((id, panel)));
        let changes = block2::RcBlock::new(move |context: *mut AnyObject| {
            // SAFETY: as `fade_out`'s, and `+functionWithName:` takes a
            // CAMediaTimingFunctionName (an NSString; "easeOut" is
            // `kCAMediaTimingFunctionEaseOut`'s) and returns the function
            // autoreleased, which `Retained` retains.
            unsafe {
                let _: () = msg_send![context, setDuration: 0.2f64];
                let ease: Retained<AnyObject> = msg_send![
                    objc2::class!(CAMediaTimingFunction),
                    functionWithName: &*NSString::from_str("easeOut")
                ];
                let _: () = msg_send![context, setTimingFunction: &*ease];
                let animator: Retained<NSWindow> = msg_send![&*window, animator];
                animator.setFrame_display(target, false);
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
        // SAFETY: as `fade_out`'s.
        unsafe {
            let _: () = msg_send![
                objc2::class!(NSAnimationContext),
                runAnimationGroup: &*changes,
                completionHandler: &*done
            ];
        }
    }
    /// The level drag images show at (`kCGDraggingWindowLevel`), over menus.
    const DRAGGING_LEVEL: isize = 500;
    /// Move `window`'s top-left to `origin` (global, top-down from the
    /// primary screen's top) through the window server alone, as AppKit
    /// moves its drag images, without AppKit's bookkeeping for a moved
    /// window (a sixth of `setFrameOrigin:`'s cost). `CGSMoveWindow` is
    /// private API: looked up once, and false where it is missing or the
    /// `private-api` feature is off (App Store builds).
    pub(crate) fn server_move(window: &NSWindow, origin: NSPoint) -> bool {
        #[cfg(feature = "private-api")]
        {
            use std::ffi::{c_char, c_void};
            type Connection = unsafe extern "C" fn() -> i32;
            type Move = unsafe extern "C" fn(i32, u32, *const NSPoint) -> i32;
            // SAFETY: libSystem's `dlsym` (dlfcn.h).
            unsafe extern "C" {
                fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
            }
            thread_local! {
                static MOVE: Option<(Connection, Move)> = {
                    // `RTLD_DEFAULT`: every image loaded (AppKit loads
                    // the window server's).
                    let default = -2isize as *mut c_void;
                    // SAFETY: names given as C strings; each symbol is
                    // transmuted to a function pointer only when found. The
                    // signatures are the window server's private ones
                    // (`CGSConnectionID CGSMainConnectionID(void)`,
                    // `CGError CGSMoveWindow(CGSConnectionID, CGWindowID,
                    // const CGPoint *)`) as other apps declare them: no
                    // header states them, so they are not checked here.
                    unsafe {
                        let connection = dlsym(default, c"CGSMainConnectionID".as_ptr());
                        let moved = dlsym(default, c"CGSMoveWindow".as_ptr());
                        (!connection.is_null() && !moved.is_null()).then(|| {
                            (
                                std::mem::transmute::<*mut c_void, Connection>(connection),
                                std::mem::transmute::<*mut c_void, Move>(moved),
                            )
                        })
                    }
                };
            }
            if let Some((connection, moved)) = MOVE.with(|found| *found) {
                // SAFETY: with the signatures above: the main connection, a
                // window of ours (a window number fits a CGWindowID) and a
                // point that outlives the call.
                return unsafe { moved(connection(), window.windowNumber() as u32, &origin) } == 0;
            }
        }
        let _ = (window, origin);
        false
    }
    /// The primary screen's height, to flip between AppKit's and global
    /// coordinates (AppKit's run bottom-up from its bottom).
    fn flip_height(mtm: MainThreadMarker) -> f64 {
        objc2_app_kit::NSScreen::screens(mtm)
            .firstObject()
            .map_or(0., |screen| screen.frame().size.height)
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
        // SAFETY: winit's content view, alive while its window is (borrowed
        // here); winit hands out an AppKit handle only on the main thread.
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
    /// The window's content top-left in global coordinates (top-down from
    /// the primary screen's top, in points).
    pub(crate) fn content_origin(window: &winit::window::Window) -> Option<(f64, f64)> {
        let (parent, content) = parent_of(window)?;
        let frame = parent.convertRectToScreen(content.convertRect_toView(content.bounds(), None));
        Some((
            frame.origin.x,
            flip_height(MainThreadMarker::from(&*parent)) - (frame.origin.y + frame.size.height),
        ))
    }
    pub(crate) fn window_number(window: &winit::window::Window) -> Option<isize> {
        parent_of(window).map(|(parent, _)| parent.windowNumber())
    }
    /// The front-most window at global point `(x, y)` (any app's), past
    /// drag previews.
    pub(crate) fn window_at(x: f64, y: f64) -> Option<isize> {
        let mtm = MainThreadMarker::new()?;
        let point = NSPoint::new(x, flip_height(mtm) - y);
        let mut below = 0;
        loop {
            let number =
                NSWindow::windowNumberAtPoint_belowWindowWithWindowNumber(point, below, mtm);
            if number <= 0 {
                return None;
            }
            if !PREVIEWS.with(|previews| previews.borrow().contains(&number)) {
                return Some(number);
            }
            below = number;
        }
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
        /// `Scene::set_popup`; always when `full`), and hide the rest. A drag
        /// preview's panel goes where `drag` puts it; as the drag ends
        /// (`ended`) untaken, it slides back to its source.
        #[allow(clippy::too_many_arguments)]
        pub fn update(
            &mut self,
            window: &winit::window::Window,
            scene: &Scene,
            semantics: &Semantics,
            context: &GpuContext,
            scale: f32,
            full: bool,
            drag: Option<&DragState>,
            ended: Option<DragEnded>,
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
            // A drag moving its window shows no preview: the window is it.
            let unshown = drag
                .filter(|drag| drag.moves_window)
                .and_then(|drag| drag.preview);
            let wanted: Vec<(NodeId, Rect)> = scene
                .popups()
                .iter()
                .filter(|root| Some(**root) != unshown)
                .filter_map(|root| scene.popup_bounds(*root).map(|bounds| (*root, bounds)))
                .collect();
            let mut index = 0;
            while index < self.shown.len() {
                if wanted.iter().any(|(root, _)| *root == self.shown[index].0) {
                    index += 1;
                    continue;
                }
                let (root, mut panel) = self.shown.swap_remove(index);
                zgui_gpu::present_log::log(|| format!("popup hide {root:?}"));
                if panel.preview.is_none() {
                    fade_out(panel);
                    continue;
                }
                // A drag nothing took: its preview slides back to its
                // source, as AppKit's drag images do. A taken one goes.
                let back = ended
                    .filter(|ended| {
                        ended.preview == Some(root) && !ended.accepted && !ended.outside
                    })
                    .filter(|ended| scene.contains(ended.source))
                    .zip(panel.preview.zip(panel.rect));
                match back {
                    Some((ended, ((dx, dy), rect))) => {
                        let source = scene.bounds(ended.source);
                        panel.sync();
                        panel.window.orderFront(None);
                        slide_back(
                            panel,
                            &parent,
                            &content,
                            Rect::new(source.x + dx, source.y + dy, rect.width, rect.height),
                        );
                    }
                    None => release(panel),
                }
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
                // A drag preview's panel goes where the drag puts it; its
                // frame is drawn where the scene has it.
                let preview = drag.filter(|drag| drag.preview == Some(root)).map(|drag| {
                    let laid = scene.bounds(root);
                    let (x, y) = drag.preview_origin.unwrap_or((laid.x, laid.y));
                    (rect.x - laid.x, rect.y - laid.y, x, y)
                });
                if new {
                    panel.preview = preview.map(|(dx, dy, ..)| (dx, dy));
                }
                let frame = match preview {
                    Some((dx, dy, x, y)) => Rect::new(x + dx, y + dy, rect.width, rect.height),
                    None => rect,
                };
                let moved = panel.rect != Some(frame);
                let material = Material::of(scene, semantics, root)
                    .map(|material| material.moved(frame.x - rect.x, frame.y - rect.y));
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
                panel.back(material, frame);
                panel.place(&parent, &content, frame);
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
                    if panel.preview.is_some() {
                        // Over every window, the pointer passing through it,
                        // as AppKit's drag images.
                        panel.window.setIgnoresMouseEvents(true);
                        panel.window.setLevel(DRAGGING_LEVEL);
                        let number = panel.window.windowNumber();
                        PREVIEWS.with(|previews| previews.borrow_mut().push(number));
                        *ivars.parent.borrow_mut() = None;
                    } else {
                        // SAFETY: both are live windows of this process, and
                        // the panel is no ancestor of its parent (a panel
                        // has no children); `hide` takes it off again.
                        unsafe {
                            parent
                                .addChildWindow_ordered(&panel.window, NSWindowOrderingMode::Above)
                        };
                        // A child takes its parent's level; menus' is higher.
                        panel.window.setLevel(NSPopUpMenuWindowLevel);
                    }
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
        /// Move the drag preview's panel to where `drag` puts it, drawing
        /// nothing (between frames, as the pointer moves).
        pub fn follow(&mut self, window: &winit::window::Window, drag: &DragState) {
            let (Some(root), Some((x, y))) = (drag.preview, drag.preview_origin) else {
                return;
            };
            let Some((_, panel)) = self.shown.iter_mut().find(|(shown, _)| *shown == root) else {
                return;
            };
            let (Some((dx, dy)), Some(rect)) = (panel.preview, panel.rect) else {
                return;
            };
            let Some((parent, content)) = parent_of(window) else {
                return;
            };
            let frame = Rect::new(x + dx, y + dy, rect.width, rect.height);
            if panel.rect == Some(frame) {
                return;
            }
            let local = NSRect::new(
                NSPoint::new(frame.x as f64, frame.y as f64),
                NSSize::new(frame.width as f64, frame.height as f64),
            );
            let screen = parent.convertRectToScreen(content.convertRect_toView(local, None));
            let top = flip_height(MainThreadMarker::from(&*parent))
                - (screen.origin.y + screen.size.height);
            if server_move(&panel.window, NSPoint::new(screen.origin.x, top)) {
                // AppKit hears where it went as the drag ends (`hide`).
                panel.moved = Some((parent, content, frame));
            } else {
                // Its size is the same: only its origin moves.
                panel.window.setFrameOrigin(screen.origin);
                panel.view.ivars().origin.set((frame.x, frame.y));
            }
            panel.rect = Some(frame);
        }
        /// The drag preview's last frame, as a drag image with the pointer in
        /// it (to hand the drag to AppKit).
        pub fn preview_image(&self, drag: &DragState) -> Option<crate::native_drag::DragImage> {
            let root = drag.preview?;
            let (_, panel) = self.shown.iter().find(|(shown, _)| *shown == root)?;
            let rect = panel.rect?;
            let (width, height, bgra) = panel.renderer.as_ref()?.shown_pixels()?;
            Some(crate::native_drag::DragImage {
                width,
                height,
                bgra,
                scale: panel.scale,
                grab: (drag.x - rect.x, drag.y - rect.y),
            })
        }
        /// Show the drag preview, or not (`None`: AppKit shows the drag's
        /// image meanwhile); shown again with the drag's pointer at `at` (the
        /// parent's coordinates).
        pub fn show_preview(
            &mut self,
            window: &winit::window::Window,
            drag: &DragState,
            at: Option<(f32, f32)>,
        ) {
            let Some(root) = drag.preview else {
                return;
            };
            let Some((_, panel)) = self.shown.iter_mut().find(|(shown, _)| *shown == root) else {
                return;
            };
            match at {
                None => panel.window.orderOut(None),
                Some((x, y)) => {
                    let (Some(rect), Some((parent, content))) = (panel.rect, parent_of(window))
                    else {
                        return;
                    };
                    let grab = (drag.x - rect.x, drag.y - rect.y);
                    let frame = Rect::new(x - grab.0, y - grab.1, rect.width, rect.height);
                    panel.place(&parent, &content, frame);
                    panel.window.orderFront(None);
                }
            }
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
            // A window closed with its popups showing: forget it, or its
            // host (and what that holds) stays registered for good, and a
            // later window given its number hears its dismissals.
            let _ = PARENTS.try_with(|parents| {
                parents
                    .borrow_mut()
                    .retain(|(_, send)| !Rc::ptr_eq(send, &self.send));
            });
        }
    }
}

#[cfg(target_os = "linux")]
mod wayland;
#[cfg(target_os = "linux")]
pub(crate) use wayland::dispatch as dispatch_wayland;

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
        /// Where it shows: its box in the parent's coordinates.
        rect: Option<Rect>,
        /// A drag preview's: its box's offset from where the scene lays the
        /// preview out (its shadow's margin), to move it with the drag.
        preview: Option<(f32, f32)>,
    }
    thread_local! {
        static HIDDEN: RefCell<Vec<Popup>> = const { RefCell::new(Vec::new()) };
        static TARGETS: RefCell<HashMap<WindowId, Rc<Target>>> = RefCell::new(HashMap::new());
        /// A popup found no window ready: make one where the event loop is.
        static WANTED: Cell<bool> = const { Cell::new(false) };
        /// Drag previews sliding back to their source, each moved by a
        /// thread of its own until it is home and hidden.
        static SLIDING: RefCell<Vec<(std::thread::JoinHandle<()>, Popup)>> =
            const { RefCell::new(Vec::new()) };
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
            // Wayland: the compositor keeps popups on its outputs, which the
            // app cannot learn; the scene places them unbounded.
            if enabled() && super::wayland::available(window) {
                return vec![Rect::new(-1e5, -1e5, 2e5, 2e5)];
            }
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
            // SAFETY: winit's open display (`parent` holds winit's
            // connection, which XInitThreads made safe to share), on its
            // event loop's thread (libXi checks the extension on it itself).
            // `pointer` and `mask`, and the bits `mask` points to, are valid
            // for the calls and not kept.
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
            // SAFETY: winit's display, still open: a grab is let go by its
            // window's `PopupHost`, which goes with the window's host, and
            // every host goes while the event loop (holding the display)
            // still runs (`WindowManager::close`, and `exiting` for those
            // still open at the end).
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
    thread_local! {
        /// The app's windows, as `window_number` named them: what
        /// `window_at` looks for.
        static OWN: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
        /// Pixels per point of global coordinates: the last window's
        /// (X11 has one scale for every window, `Xft.dpi`).
        static SCALE: Cell<f64> = const { Cell::new(1.) };
    }
    /// The window's content top-left in global coordinates (from the root
    /// window's top-left, in points), which the X server translates.
    pub(crate) fn content_origin(window: &Window) -> Option<(f64, f64)> {
        xid(window)?;
        let origin = window.inner_position().ok()?;
        let scale = window.scale_factor();
        SCALE.with(|cell| cell.set(scale));
        Some((origin.x as f64 / scale, origin.y as f64 / scale))
    }
    pub(crate) fn window_number(window: &Window) -> Option<isize> {
        let id = xid(window)?;
        OWN.with(|own| {
            let mut own = own.borrow_mut();
            if !own.contains(&id) {
                own.push(id);
            }
        });
        Some(id as isize)
    }
    /// The front-most window at global point `(x, y)` (any app's): one of
    /// the app's (`window_number`), or the top-level window of another's.
    /// The X server finds it (`XTranslateCoordinates`) by the stacking
    /// order, past windows the pointer goes through: drag previews (an
    /// empty input shape) and a compositor's overlay.
    pub(crate) fn window_at(x: f64, y: f64) -> Option<isize> {
        use x11rb::{connection::Connection, protocol::xproto::ConnectionExt};
        let scale = SCALE.with(Cell::get);
        let (x, y) = ((x * scale).round() as i16, (y * scale).round() as i16);
        let own = OWN.with(|own| own.borrow().clone());
        CONNECTION.with(|connection| {
            let connection = connection.as_ref()?;
            let root = connection.setup().roots.first()?.root;
            // Down from the root, through the window manager's frame, to
            // one of the app's windows.
            let (mut window, mut top) = (root, None);
            for _ in 0..8 {
                let child = connection
                    .translate_coordinates(root, window, x, y)
                    .ok()?
                    .reply()
                    .ok()?
                    .child;
                if child == x11rb::NONE {
                    break;
                }
                if own.contains(&child) {
                    return Some(child as isize);
                }
                top.get_or_insert(child);
                window = child;
            }
            top.map(|top| top as isize)
        })
    }
    thread_local! {
        /// While a drag moves its window: that window's frame (its
        /// top-level window, the window manager's), its frame's left and
        /// top, and each of the app's windows' top-levels.
        static MOVING: RefCell<Option<Moving>> = const { RefCell::new(None) };
    }
    struct Moving {
        window: u32,
        top: u32,
        frame: (i32, i32),
        tops: Vec<(u32, u32)>,
    }
    /// `window`'s top-level window: its ancestor that is the root's child.
    fn top_level(connection: &impl x11rb::connection::Connection, window: u32) -> Option<u32> {
        use x11rb::protocol::xproto::ConnectionExt;
        let mut window = window;
        for _ in 0..8 {
            let tree = connection.query_tree(window).ok()?.reply().ok()?;
            if tree.parent == tree.root || tree.parent == x11rb::NONE {
                return Some(window);
            }
            window = tree.parent;
        }
        None
    }
    /// `MOVING` for `window`, found once per drag.
    fn moving<T>(window: u32, read: impl FnOnce(&Moving) -> T) -> Option<T> {
        use x11rb::connection::Connection;
        MOVING.with(|moving| {
            let mut moving = moving.borrow_mut();
            if moving.as_ref().is_none_or(|moving| moving.window != window) {
                *moving = CONNECTION.with(|connection| {
                    let connection = connection.as_ref()?;
                    let root = connection.setup().roots.first()?.root;
                    let extents = crate::x11_work_area::frame_extents(connection, root, window)
                        .unwrap_or_default();
                    let own = OWN.with(|own| own.borrow().clone());
                    Some(Moving {
                        window,
                        top: top_level(connection, window)?,
                        frame: (extents.0 as i32, extents.2 as i32),
                        tops: own
                            .into_iter()
                            .filter_map(|own| Some((top_level(connection, own)?, own)))
                            .collect(),
                    })
                });
            }
            moving.as_ref().map(read)
        })
    }
    /// The pointer in global coordinates (points from the root's top-left),
    /// as the X server has it.
    pub(crate) fn global_pointer() -> Option<(f64, f64)> {
        use x11rb::{connection::Connection, protocol::xproto::ConnectionExt};
        let scale = SCALE.with(Cell::get);
        CONNECTION.with(|connection| {
            let connection = connection.as_ref()?;
            let root = connection.setup().roots.first()?.root;
            let pointer = connection.query_pointer(root).ok()?.reply().ok()?;
            Some((pointer.root_x as f64 / scale, pointer.root_y as f64 / scale))
        })
    }
    /// Put `window`'s content top-left at global point `to`, as a drag of
    /// its only tab does (`View::drag_moves_window`): its frame moved (a
    /// request the window manager carries out), drawing nothing.
    pub(crate) fn move_window(window: &Window, to: (f64, f64)) {
        use x11rb::{
            connection::Connection,
            protocol::xproto::{ConfigureWindowAux, ConnectionExt},
        };
        let Some(id) = xid(window) else {
            return;
        };
        let Some((left, top)) = moving(id, |moving| moving.frame) else {
            return;
        };
        let scale = window.scale_factor();
        let (x, y) = (
            (to.0 * scale).round() as i32 - left,
            (to.1 * scale).round() as i32 - top,
        );
        CONNECTION.with(|connection| {
            if let Some(connection) = connection {
                let _ = connection.configure_window(id, &ConfigureWindowAux::new().x(x).y(y));
                let _ = connection.flush();
            }
        });
    }
    /// The drag that moved a window ended.
    pub(crate) fn settle_window() {
        MOVING.with(|moving| moving.borrow_mut().take());
    }
    /// The front-most window at global point `(x, y)` below window
    /// `number`, which a drag is moving under the pointer: one of the app's
    /// (`window_number`), or another's top-level window. The root's
    /// children, top down, as the stacking order has them, past windows not
    /// shown and override-redirect ones (popups, drag previews).
    pub(crate) fn window_below(x: f64, y: f64, number: isize) -> Option<isize> {
        use x11rb::{
            connection::Connection,
            protocol::xproto::{ConnectionExt, MapState},
        };
        let scale = SCALE.with(Cell::get);
        let (x, y) = ((x * scale).round() as i32, (y * scale).round() as i32);
        let (moved, tops) = moving(number as u32, |moving| (moving.top, moving.tops.clone()))?;
        CONNECTION.with(|connection| {
            let connection = connection.as_ref()?;
            let root = connection.setup().roots.first()?.root;
            let children = connection.query_tree(root).ok()?.reply().ok()?.children;
            // Asked all at once: one round trip, however many windows.
            let asked: Vec<_> = children
                .iter()
                .rev()
                .filter(|child| **child != moved)
                .filter_map(|child| {
                    Some((
                        *child,
                        connection.get_window_attributes(*child).ok()?,
                        connection.get_geometry(*child).ok()?,
                    ))
                })
                .collect();
            let mut found = None;
            for (child, attributes, geometry) in asked {
                let (Ok(attributes), Ok(geometry)) = (attributes.reply(), geometry.reply()) else {
                    continue;
                };
                if found.is_some()
                    || attributes.map_state != MapState::VIEWABLE
                    || attributes.override_redirect
                {
                    continue;
                }
                let border = 2 * geometry.border_width as i32;
                let (left, top) = (geometry.x as i32, geometry.y as i32);
                if (left..left + geometry.width as i32 + border).contains(&x)
                    && (top..top + geometry.height as i32 + border).contains(&y)
                {
                    found = Some(child);
                }
            }
            let found = found?;
            let own = tops.iter().find(|(top, _)| *top == found);
            Some(own.map_or(found, |(_, own)| *own) as isize)
        })
    }
    /// Whether a popup found no window ready (`make` one).
    pub(crate) fn wanted() -> bool {
        reap();
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
                    preview: None,
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
        /// A Wayland window's popups.
        wayland: super::wayland::Popups,
    }
    impl PopupHost {
        pub fn new(send: impl Fn(PopupInput) + 'static) -> Self {
            let send: Sink = Rc::new(send);
            Self {
                shown: Vec::new(),
                wayland: super::wayland::Popups::new(send.clone()),
                send,
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
            !self.shown.is_empty() || self.wayland.showing()
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
            drag: Option<&zgui::input::DragState>,
            ended: Option<zgui::input::DragEnded>,
        ) -> Result<(), GpuError> {
            reap();
            if !x11(parent) {
                return self.wayland.update(parent, scene, context, scale, full);
            }
            if self.shown.is_empty() && scene.popups().is_empty() {
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
                // A drag nothing took: its preview slides back to its
                // source, as drag images do. A taken one goes.
                let back = ended
                    .filter(|ended| {
                        ended.preview == Some(root) && !ended.accepted && !ended.outside
                    })
                    .filter(|ended| scene.contains(ended.source))
                    .zip(popup.preview.zip(popup.rect))
                    .zip(parent.inner_position().ok());
                match back {
                    Some(((ended, ((dx, dy), rect)), origin)) => {
                        let source = scene.bounds(ended.source);
                        let at = |x: f32, y: f32| {
                            PhysicalPosition::new(
                                origin.x + (x * scale).round() as i32,
                                origin.y + (y * scale).round() as i32,
                            )
                        };
                        let from = at(rect.x, rect.y);
                        slide_back(popup, from, at(source.x + dx, source.y + dy));
                    }
                    None => release(popup),
                }
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
                // A drag preview's window goes where the drag puts it (and
                // `follow` moves it between frames); its frame is drawn
                // where the scene has it.
                let preview = drag.filter(|drag| drag.preview == Some(root)).map(|drag| {
                    let laid = scene.bounds(root);
                    let (x, y) = drag.preview_origin.unwrap_or((laid.x, laid.y));
                    (rect.x - laid.x, rect.y - laid.y, x, y)
                });
                if new {
                    popup.preview = preview.map(|(dx, dy, ..)| (dx, dy));
                    // The pointer goes through it, to what is under it.
                    if popup.preview.is_some() {
                        pass_through(&popup.window, true);
                    }
                }
                let frame = match preview {
                    Some((dx, dy, x, y)) => Rect::new(x + dx, y + dy, rect.width, rect.height),
                    None => rect,
                };
                let resized = popup
                    .rect
                    .is_none_or(|old| (old.width, old.height) != (rect.width, rect.height));
                if resized {
                    // The window to the popup's box, in whole device pixels,
                    // before its surface draws at that size.
                    let size = PhysicalSize::new(
                        (rect.width * scale).round() as u32,
                        (rect.height * scale).round() as u32,
                    );
                    let _ = popup.window.request_inner_size(size);
                }
                // Popups are small: one that changed is drawn whole. One that
                // only moved (a drag preview) is not drawn again.
                let revision = scene.layer_revision(root);
                if new || full || resized || popup.revision != Some(revision) {
                    popup.revision = Some(revision);
                    zgui_gpu::present_log::log(|| format!("popup draw {root:?}"));
                    renderer.repaint_all();
                    renderer.render(scene, &[])?;
                    renderer.present()?;
                }
                // Onto the screen once its frame is there.
                if popup.rect != Some(frame) {
                    place(&popup.window, origin, frame, scale);
                }
                let rect = frame;
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
        /// Move the drag preview's window to where `drag` puts it, drawing
        /// nothing (between frames, as the pointer moves): the X server
        /// moves it (`XMoveWindow`).
        pub fn follow(&mut self, parent: &Window, drag: &zgui::input::DragState) {
            let (Some(root), Some((x, y))) = (drag.preview, drag.preview_origin) else {
                return;
            };
            let Some((_, popup)) = self.shown.iter_mut().find(|(shown, _)| *shown == root) else {
                return;
            };
            let (Some((dx, dy)), Some(rect), Ok(origin)) =
                (popup.preview, popup.rect, parent.inner_position())
            else {
                return;
            };
            let frame = Rect::new(x + dx, y + dy, rect.width, rect.height);
            if popup.rect == Some(frame) {
                return;
            }
            place(&popup.window, origin, frame, parent.scale_factor() as f32);
            popup.rect = Some(frame);
            TARGETS.with(|targets| {
                if let Some(target) = targets.borrow().get(&popup.window.id()) {
                    target.origin.set((frame.x, frame.y));
                }
            });
        }
        pub fn preview_image(
            &self,
            _: &zgui::input::DragState,
        ) -> Option<crate::native_drag::DragImage> {
            None
        }
        pub fn show_preview(
            &mut self,
            _: &Window,
            _: &zgui::input::DragState,
            _: Option<(f32, f32)>,
        ) {
        }
        pub fn hide_all(&mut self, parent: &Window) {
            for (_, popup) in self.shown.drain(..) {
                release(popup);
            }
            self.wayland.hide_all(parent);
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
    /// Show `window` at `frame` (the parent's points), the parent's content
    /// at `origin` (physical, on the root).
    fn place(window: &Window, origin: PhysicalPosition<i32>, frame: Rect, scale: f32) {
        window.set_outer_position(PhysicalPosition::new(
            origin.x + (frame.x * scale).round() as i32,
            origin.y + (frame.y * scale).round() as i32,
        ));
    }
    /// Hide `popup` (parked off screen, unmapped), and keep it made if few
    /// are.
    fn release(mut popup: Popup) {
        popup.window.set_outer_position(PARKED);
        popup.window.set_visible(false);
        if popup.preview.take().is_some() {
            pass_through(&popup.window, false);
        }
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
    /// Whether the pointer goes through `window` (an empty input shape) to
    /// the window under it, or (`on` false) it takes it again.
    fn pass_through(window: &Window, on: bool) {
        use x11rb::{
            connection::Connection,
            protocol::shape::{self, SK, SO},
        };
        let Some(id) = xid(window) else {
            return;
        };
        CONNECTION.with(|connection| {
            let Some(connection) = connection else {
                return;
            };
            let _ = if on {
                shape::rectangles(
                    connection,
                    SO::SET,
                    SK::INPUT,
                    x11rb::protocol::xproto::ClipOrdering::UNSORTED,
                    id,
                    0,
                    0,
                    &[],
                )
            } else {
                // No mask: the window's whole box again.
                shape::mask(connection, SO::SET, SK::INPUT, id, 0, 0, x11rb::NONE)
            };
            let _ = connection.flush();
        });
    }
    /// How long a drag preview takes to slide back (as AppKit's do).
    const SLIDE: std::time::Duration = std::time::Duration::from_millis(200);
    /// Slide a drag preview's `popup` from `from` to `to` (on the root,
    /// physical), easing out, then hide it: moved by a thread with a
    /// connection of its own, the parent drawing nothing; put back in the
    /// pool once home (`reap`).
    fn slide_back(popup: Popup, from: PhysicalPosition<i32>, to: PhysicalPosition<i32>) {
        use x11rb::{
            connection::Connection, protocol::xproto::ConnectionExt, wrapper::ConnectionExt as _,
        };
        let Some(id) = xid(&popup.window) else {
            return release(popup);
        };
        let slide = std::thread::spawn(move || {
            let Ok((connection, _)) = x11rb::connect(None) else {
                return;
            };
            let start = std::time::Instant::now();
            loop {
                let t = (start.elapsed().as_secs_f32() / SLIDE.as_secs_f32()).min(1.);
                let ease = 1. - (1. - t) * (1. - t);
                let along = |from: i32, to: i32| from + ((to - from) as f32 * ease).round() as i32;
                let aux = x11rb::protocol::xproto::ConfigureWindowAux::new()
                    .x(along(from.x, to.x))
                    .y(along(from.y, to.y));
                let _ = connection.configure_window(id, &aux);
                let _ = connection.flush();
                if t >= 1. {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(8));
            }
            // Hidden before the window is shown again from the pool.
            let _ = connection.unmap_window(id);
            let _ = connection.sync();
        });
        SLIDING.with(|sliding| sliding.borrow_mut().push((slide, popup)));
    }
    /// Put back in the pool the previews home from sliding.
    fn reap() {
        let home: Vec<Popup> = SLIDING.with(|sliding| {
            let mut sliding = sliding.borrow_mut();
            if sliding.is_empty() {
                return Vec::new();
            }
            let (home, still) = sliding
                .drain(..)
                .partition(|(slide, _)| slide.is_finished());
            *sliding = still;
            home.into_iter().map(|(_, popup)| popup).collect()
        });
        home.into_iter().for_each(release);
    }
}
