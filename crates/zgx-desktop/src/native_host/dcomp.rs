//! A window's DirectComposition tree, and its native views in it (Windows).
//!
//! The window's target root holds, from the bottom:
//!
//! - the base plane: the window renderer's swap chain;
//! - per native view, in paint order, a stack of nested visuals ("cuts"),
//!   each clipped to one shape: the node's rectangular clip, then each
//!   rounded clip (its own corners first). Cuts never move; the innermost
//!   holds the page's visual (WebView2's root visual target) at the node's
//!   frame, the outermost its opacity;
//! - the overlay root, holding the overlay plane's swap chain visual at the
//!   overlay paint's box while anything paints there.
//!
//! Everything is in physical pixels. Geometry changes commit with a frame's
//! presents (`Transaction`), right after them, so they land in the same
//! composition. Pages get no window messages of their own for the pointer:
//! the window's subclass routes them (`PlaneSplit::route`), and takes the
//! keyboard back from a page when zgx content is clicked.
use super::{Shape, shapes, webview2};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM},
    Graphics::{
        DirectComposition::{
            DCOMPOSITION_BORDER_MODE_SOFT, DCompositionCreateDevice2, IDCompositionDevice,
            IDCompositionEffectGroup, IDCompositionRectangleClip, IDCompositionTarget,
            IDCompositionVisual,
        },
        Gdi::ScreenToClient,
    },
    UI::{
        HiDpi::GetDpiForWindow,
        Input::KeyboardAndMouse::{GetFocus, ReleaseCapture, SetCapture, SetFocus},
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::{
            HTCLIENT, IsChild, SetCursor, WM_CAPTURECHANGED, WM_EXITSIZEMOVE, WM_LBUTTONDBLCLK,
            WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDBLCLK, WM_MBUTTONDOWN, WM_MBUTTONUP,
            WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_MOVE, WM_NCDESTROY, WM_RBUTTONDBLCLK,
            WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SETCURSOR, WM_XBUTTONDBLCLK, WM_XBUTTONDOWN,
            WM_XBUTTONUP,
        },
    },
};
use zgx::{
    input::InputDispatcher,
    native_view::{
        NativeContent, NativePlacement, NativeViews, PageReport, PlaneSplit, PointerRoute,
        PointerStep,
    },
    scene::{NodeId, Rect, Scene},
    widgets::Ui,
};

/// Cut levels around a native view: its rectangular clip, its own corners
/// and its two nearest rounded clipping ancestors (`RoundedClip`).
const NATIVE_CUTS: usize = 4;
const SUBCLASS_ID: usize = 0x7a67_7577; // "zguw"
/// The pointer left the window (TrackMouseEvent's; not in the bindings' module).
const WM_MOUSELEAVE: u32 = 0x02a3;

pub(crate) fn hwnd(window: &winit::window::Window) -> Option<HWND> {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(HWND(handle.hwnd.get() as *mut _)),
        _ => None,
    }
}
fn scale_of(hwnd: HWND) -> f32 {
    // SAFETY: reads our own window's DPI.
    unsafe { GetDpiForWindow(hwnd) }.max(96) as f32 / 96.
}

/// A window's composition: its device, target and the visuals zgx owns.
pub(crate) struct Composition {
    pub device: IDCompositionDevice,
    _target: IDCompositionTarget,
    root: IDCompositionVisual,
    /// The base plane's visual (the window renderer presents into it).
    pub base: IDCompositionVisual,
}
impl Composition {
    pub fn new(window: &winit::window::Window) -> Result<Self, String> {
        let hwnd = hwnd(window).ok_or("not a Win32 window")?;
        fn error(what: &'static str) -> impl Fn(windows::core::Error) -> String {
            move |e| format!("{what}: {e}")
        }
        // SAFETY: plain DirectComposition calls on objects made here, on
        // the window's thread.
        unsafe {
            let device: IDCompositionDevice =
                DCompositionCreateDevice2(None).map_err(error("DCompositionCreateDevice2"))?;
            let target = device
                .CreateTargetForHwnd(hwnd, false)
                .map_err(error("CreateTargetForHwnd"))?;
            let root = device.CreateVisual().map_err(error("CreateVisual"))?;
            let base = device.CreateVisual().map_err(error("CreateVisual"))?;
            root.AddVisual(&base, true, None)
                .map_err(error("AddVisual"))?;
            target.SetRoot(&root).map_err(error("SetRoot"))?;
            Ok(Self {
                device,
                _target: target,
                root,
                base,
            })
        }
    }
    fn commit(&self) {
        // SAFETY: our device, on its thread.
        let _ = unsafe { self.device.Commit() };
    }
}

/// One commit of the window's tree: made when it drops.
pub(crate) struct Transaction(Rc<Composition>);
impl Transaction {
    pub fn begin(composition: Rc<Composition>) -> Self {
        Self(composition)
    }
}
impl Drop for Transaction {
    fn drop(&mut self) {
        self.0.commit();
    }
}

/// Where input goes, asked from the window procedure (possibly while zgx
/// holds its state).
pub(crate) struct Router {
    pub scene: Weak<RefCell<Scene>>,
    pub input: InputDispatcher,
    pub natives: NativeViews,
    /// The pointer over native views (zgx's leave, pages' pointers).
    pub pointer: zgx::native_view::NativePointer,
}
impl Router {
    /// The pointer moved to window point `(x, y)`: what zgx hears.
    pub fn track(&self, x: f32, y: f32, held: bool) -> (PointerStep, PageReport) {
        let route = self.route(x, y);
        self.pointer.moved(&self.natives, route, held, x, y)
    }
    fn route(&self, x: f32, y: f32) -> PointerRoute {
        let split = self.natives.split();
        let Some(scene) = self.scene.upgrade() else {
            return PointerRoute::Ui;
        };
        match scene.try_borrow() {
            Ok(scene) => split.route(&scene, &self.input, x, y),
            // Mid-frame: the front-most page whose visible box holds the point.
            Err(_) => split
                .natives()
                .iter()
                .rev()
                .find(|placement| {
                    placement.visible().is_some_and(|r| {
                        x >= r.x && y >= r.y && x < r.x + r.width && y < r.y + r.height
                    })
                })
                .map_or(PointerRoute::Ui, |placement| {
                    PointerRoute::Native(placement.node)
                }),
        }
    }
    /// The page of `node`, if it shows one that is ready.
    fn page(&self, node: NodeId) -> Option<Rc<webview2::Page>> {
        match self.natives.content(node)? {
            NativeContent::Web(web) => webview2::page_of(&web),
            _ => None,
        }
    }
}

/// What the window procedure follows of the pointer.
struct Pointer {
    router: Rc<Router>,
    /// A press a page took: it hears every move and button until released.
    pressed: Cell<Option<NodeId>>,
    /// A press on zgx content: its moves are zgx's, wherever, while a
    /// button is held (the messages' own MK_ flags say: a release the window
    /// never hears, capture lost to another window, ends it as well).
    ui_press: Cell<bool>,
    /// The page the pointer is over, which heard its last move.
    hovered: Cell<Option<NodeId>>,
    /// The window still has the subclass (its destruction removes it).
    attached: Cell<bool>,
}
impl Pointer {
    /// Send a mouse message to the page of `node`, at client point
    /// `(x, y)` (physical pixels).
    fn send(&self, hwnd: HWND, node: NodeId, message: u32, wparam: WPARAM, x: i32, y: i32) {
        let Some(page) = self.router.page(node) else {
            return;
        };
        let scale = scale_of(hwnd);
        let origin = self
            .router
            .natives
            .split()
            .native(node)
            .map(|placement| frame_px(placement.frame, scale))
            .map_or((0, 0), |frame| (frame.0, frame.1));
        page.send_mouse(message, wparam, x - origin.0, y - origin.1);
    }
    fn leave(&self, hwnd: HWND) {
        if let Some(node) = self.hovered.take() {
            if let Some(page) = self.router.page(node) {
                page.hovered.set(false);
            }
            self.send(hwnd, node, WM_MOUSELEAVE, WPARAM(0), 0, 0);
        }
    }
    fn hover(&self, hwnd: HWND, node: NodeId) {
        if self.hovered.get() != Some(node) {
            self.leave(hwnd);
            self.hovered.set(Some(node));
            if let Some(page) = self.router.page(node) {
                page.hovered.set(true);
            }
        }
    }
    /// Whether a press of zgx's holds the pointer, by the buttons a mouse
    /// message says are down.
    fn ui_held(&self, wparam: WPARAM) -> bool {
        if wparam.0 & BUTTONS == 0 {
            self.ui_press.set(false);
        }
        self.ui_press.get()
    }
    /// Where the pointer at client point `(x, y)` goes.
    fn route(&self, hwnd: HWND, x: i32, y: i32) -> PointerRoute {
        let scale = scale_of(hwnd);
        self.router.route(x as f32 / scale, y as f32 / scale)
    }
    /// A window message, before winit: whether it was a page's (winit then
    /// never hears it).
    fn handle(&self, hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> bool {
        let client = || {
            (
                (lparam.0 & 0xffff) as i16 as i32,
                ((lparam.0 >> 16) & 0xffff) as i16 as i32,
            )
        };
        match message {
            WM_MOUSEMOVE => {
                let (x, y) = client();
                if let Some(node) = self.pressed.get() {
                    self.send(hwnd, node, message, wparam, x, y);
                    return true;
                }
                if self.ui_held(wparam) {
                    return false;
                }
                match self.route(hwnd, x, y) {
                    PointerRoute::Native(node) => {
                        self.hover(hwnd, node);
                        self.send(hwnd, node, message, wparam, x, y);
                    }
                    PointerRoute::Ui => self.leave(hwnd),
                }
                // zgx hears it too: its host follows the pointer (a leave).
                false
            }
            WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN
            | WM_LBUTTONDBLCLK | WM_RBUTTONDBLCLK | WM_MBUTTONDBLCLK | WM_XBUTTONDBLCLK => {
                let (x, y) = client();
                if let Some(node) = self.pressed.get() {
                    self.send(hwnd, node, message, wparam, x, y);
                    return true;
                }
                // Another button of a press of zgx's still held: still zgx's.
                let others = wparam.0 & BUTTONS & !button_of(message, wparam);
                if !(self.ui_press.get() && others != 0) {
                    self.ui_press.set(false);
                }
                if !self.ui_press.get()
                    && let PointerRoute::Native(node) = self.route(hwnd, x, y)
                {
                    self.pressed.set(Some(node));
                    // SAFETY: our own window, on its thread.
                    unsafe { SetCapture(hwnd) };
                    self.send(hwnd, node, message, wparam, x, y);
                    return true;
                }
                self.ui_press.set(true);
                // A click on zgx content takes the keyboard back from a page.
                // SAFETY: reads and sets focus within our own window.
                unsafe {
                    let focus = GetFocus();
                    if !focus.is_invalid() && focus != hwnd && IsChild(hwnd, focus).as_bool() {
                        let _ = SetFocus(Some(hwnd));
                    }
                }
                false
            }
            WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP | WM_XBUTTONUP => {
                let (x, y) = client();
                if let Some(node) = self.pressed.get() {
                    self.send(hwnd, node, message, wparam, x, y);
                    // Released when no button is held (MK_ flags).
                    if wparam.0 & BUTTONS == 0 {
                        self.pressed.set(None);
                        // SAFETY: releases the capture taken for the press.
                        let _ = unsafe { ReleaseCapture() };
                    }
                    return true;
                }
                self.ui_held(wparam);
                false
            }
            WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
                // Wheel messages carry screen coordinates.
                let (x, y) = client();
                let mut point = POINT { x, y };
                // SAFETY: our own window; writes a local.
                let _ = unsafe { ScreenToClient(hwnd, &mut point) };
                let node = match self.pressed.get() {
                    Some(node) => Some(node),
                    None if !self.ui_held(wparam) => match self.route(hwnd, point.x, point.y) {
                        PointerRoute::Native(node) => Some(node),
                        PointerRoute::Ui => None,
                    },
                    None => None,
                };
                if let Some(node) = node {
                    self.send(hwnd, node, message, wparam, point.x, point.y);
                    return true;
                }
                false
            }
            WM_MOUSELEAVE => {
                if self.pressed.get().is_none() {
                    self.leave(hwnd);
                }
                false
            }
            WM_CAPTURECHANGED => {
                if lparam.0 as *mut std::ffi::c_void != hwnd.0 {
                    self.pressed.set(None);
                    self.ui_press.set(false);
                }
                false
            }
            // A move or resize loop (a window drag) took the pointer.
            WM_EXITSIZEMOVE => {
                self.pressed.set(None);
                self.ui_press.set(false);
                false
            }
            WM_MOVE => {
                for node in self.router.natives.nodes() {
                    if let Some(page) = self.router.page(node) {
                        page.parent_moved();
                    }
                }
                false
            }
            WM_SETCURSOR if (lparam.0 & 0xffff) as u32 == HTCLIENT => {
                let node = self.pressed.get().or(self.hovered.get());
                if let Some(cursor) = node
                    .and_then(|node| self.router.page(node))
                    .and_then(|page| page.cursor())
                {
                    // SAFETY: a cursor the page handed us.
                    unsafe { SetCursor(Some(cursor)) };
                    return true;
                }
                false
            }
            _ => false,
        }
    }
}

/// # Safety
///
/// Called by comctl32 alone, as the window's subclass installed by
/// `Surfaces::new`: `data` is the `Rc<Pointer>` it leaked for it, whose
/// reference the subclass holds until it is removed.
unsafe extern "system" fn subclass(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    data: usize,
) -> LRESULT {
    // SAFETY: `data` is the `Rc` `Surfaces::new` leaked for this subclass,
    // whose reference stays until the subclass is removed (here or when the
    // surfaces go). A reference of this call's own keeps it alive while the
    // message is handled: handling it sends messages (`SetFocus`, a page's
    // input) that run winit's procedure and the app, which may drop the
    // surfaces meanwhile.
    let pointer = unsafe {
        Rc::increment_strong_count(data as *const Pointer);
        Rc::from_raw(data as *const Pointer)
    };

    if message == WM_NCDESTROY {
        pointer.attached.set(false);
        // SAFETY: removes this subclass and takes back its reference: it
        // runs, so it is installed, and the surfaces take the reference
        // back only once they have removed it.
        unsafe {
            let _ = RemoveWindowSubclass(hwnd, Some(subclass), SUBCLASS_ID);
            drop(Rc::from_raw(data as *const Pointer));
        }
    } else if pointer.handle(hwnd, message, wparam, lparam) {
        return LRESULT(if message == WM_SETCURSOR { 1 } else { 0 });
    }
    // SAFETY: forwards to the next procedure in the chain.
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

/// The MK_ flags of the mouse buttons.
const BUTTONS: usize = 0x73;
/// The MK_ flag of the button a button message is for.
fn button_of(message: u32, wparam: WPARAM) -> usize {
    match message {
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => 0x01,
        WM_RBUTTONDOWN | WM_RBUTTONDBLCLK => 0x02,
        WM_MBUTTONDOWN | WM_MBUTTONDBLCLK => 0x10,
        // XBUTTON1 or XBUTTON2, in the high word.
        _ if (wparam.0 >> 16) & 0xffff == 2 => 0x40,
        _ => 0x20,
    }
}

/// A box in physical pixels: left, top, right, bottom.
fn frame_px(rect: Rect, scale: f32) -> (i32, i32, i32, i32) {
    let left = (rect.x * scale).round() as i32;
    let top = (rect.y * scale).round() as i32;
    let right = ((rect.x + rect.width) * scale).round() as i32;
    let bottom = ((rect.y + rect.height) * scale).round() as i32;
    (left, top, right.max(left), bottom.max(top))
}

/// A cut visual and its clip.
struct Cut {
    visual: IDCompositionVisual,
    clip: Option<IDCompositionRectangleClip>,
    /// What it clips to, at the scale it was set for.
    shape: Option<(Option<Shape>, f32)>,
}
impl Cut {
    fn cut(&mut self, device: &IDCompositionDevice, shape: Option<Shape>, scale: f32) {
        if self.shape == Some((shape, scale)) {
            return;
        }
        self.shape = Some((shape, scale));
        // SAFETY: our visuals and clips, on the window's thread.
        unsafe {
            let Some(shape) = shape else {
                let _ = self.visual.SetClip(
                    None::<&windows::Win32::Graphics::DirectComposition::IDCompositionClip>,
                );
                self.clip = None;
                return;
            };
            if self.clip.is_none() {
                let Ok(clip) = device.CreateRectangleClip() else {
                    return;
                };
                let _ = self.visual.SetClip(&clip);
                self.clip = Some(clip);
            }
            let clip = self.clip.as_ref().expect("made above");
            let (rect, radii) = match shape {
                Shape::Rect(rect) => (rect, [0.; 4]),
                Shape::Round(round) => (round.rect, round.radii),
            };
            let [tl, tr, br, bl] = radii.map(|radius| radius.max(0.) * scale);
            let _ = clip.SetLeft2(rect.x * scale);
            let _ = clip.SetTop2(rect.y * scale);
            let _ = clip.SetRight2((rect.x + rect.width) * scale);
            let _ = clip.SetBottom2((rect.y + rect.height) * scale);
            let _ = clip.SetTopLeftRadiusX2(tl);
            let _ = clip.SetTopLeftRadiusY2(tl);
            let _ = clip.SetTopRightRadiusX2(tr);
            let _ = clip.SetTopRightRadiusY2(tr);
            let _ = clip.SetBottomRightRadiusX2(br);
            let _ = clip.SetBottomRightRadiusY2(br);
            let _ = clip.SetBottomLeftRadiusX2(bl);
            let _ = clip.SetBottomLeftRadiusY2(bl);
        }
    }
}

struct Clip {
    cuts: Vec<Cut>,
    /// The outermost cut's opacity.
    effect: Option<IDCompositionEffectGroup>,
    opacity: f32,
    /// The visual holding the page, at its frame.
    inner: IDCompositionVisual,
    web: Option<zgx::web_view::WebView>,
    page: Option<Rc<webview2::Page>>,
    placed: Option<(NativePlacement, f32)>,
    /// Shown: the outermost cut is in the tree.
    shown: bool,
}
impl Clip {
    fn outer(&self) -> &IDCompositionVisual {
        &self.cuts[0].visual
    }
}

pub(crate) struct Surfaces {
    composition: Rc<Composition>,
    hwnd: HWND,
    pointer: Rc<Pointer>,
    clips: Vec<(NodeId, Clip)>,
    /// The top of the tree; holds the overlay plane's visual while shown.
    overlay_root: IDCompositionVisual,
    overlay: Option<IDCompositionVisual>,
    /// The overlay's offset as last shown (None: hidden).
    overlay_shown: Option<Option<(i32, i32)>>,
    /// Paint order of the shown pages, as the tree has them.
    order: Vec<NodeId>,
    /// The page that held keyboard focus at the last check.
    focus: Option<NodeId>,
}

impl Surfaces {
    pub fn new(
        window: &winit::window::Window,
        composition: Rc<Composition>,
        router: Router,
    ) -> Result<Self, String> {
        let hwnd = hwnd(window).ok_or("not a Win32 window")?;
        // SAFETY: a visual of our device, added to our root.
        let overlay_root = unsafe {
            let visual = composition
                .device
                .CreateVisual()
                .map_err(|e| e.to_string())?;
            composition
                .root
                .AddVisual(&visual, false, None)
                .map_err(|e| e.to_string())?;
            visual
        };
        let pointer = Rc::new(Pointer {
            router: Rc::new(router),
            pressed: Cell::new(None),
            ui_press: Cell::new(false),
            hovered: Cell::new(None),
            attached: Cell::new(true),
        });
        // SAFETY: the subclass runs on this (the window's) thread; its
        // reference is released when it is removed.
        unsafe {
            let data = Rc::into_raw(pointer.clone()) as usize;
            if !SetWindowSubclass(hwnd, Some(subclass), SUBCLASS_ID, data).as_bool() {
                drop(Rc::from_raw(data as *const Pointer));
                return Err("the window's pointer could not be followed".into());
            }
        }
        Ok(Self {
            composition,
            hwnd,
            pointer,
            clips: Vec::new(),
            overlay_root,
            overlay: None,
            overlay_shown: None,
            order: Vec::new(),
            focus: None,
        })
    }
    pub fn router(&self) -> &Router {
        &self.pointer.router
    }
    fn scale(&self) -> f32 {
        scale_of(self.hwnd)
    }
    /// The overlay plane's renderer, presenting into a visual of its own
    /// (shown by `show_overlay`).
    pub fn overlay_renderer(
        &mut self,
        context: &zgx_gpu::GpuContext,
        scale: f32,
    ) -> Result<zgx_gpu::GpuRenderer, zgx_gpu::GpuError> {
        if let Some(old) = self.overlay.take() {
            // SAFETY: removes our visual from our tree.
            let _ = unsafe { self.overlay_root.RemoveVisual(&old) };
        }
        self.overlay_shown = None;
        // SAFETY: a visual of our device.
        let visual = unsafe { self.composition.device.CreateVisual() }
            .map_err(|e| zgx_gpu::GpuError(e.to_string()))?;
        let renderer = zgx_gpu::GpuRenderer::for_overlay_visual(
            visual.clone(),
            self.composition.device.clone(),
            context,
            scale,
        )?;
        self.overlay = Some(visual);
        Ok(renderer)
    }
    /// Whether `split` moves anything native relative to `previous`.
    pub fn differs(&self, split: &PlaneSplit, previous: &PlaneSplit) -> bool {
        split.natives() != previous.natives()
    }
    /// Show the overlay plane at `rect` (whole device pixels), or hide it.
    pub fn show_overlay(&mut self, rect: Option<Rect>) {
        let Some(overlay) = &self.overlay else {
            return;
        };
        let scale = self.scale();
        let offset = rect.map(|rect| {
            (
                (rect.x * scale).round() as i32,
                (rect.y * scale).round() as i32,
            )
        });
        if self.overlay_shown == Some(offset) {
            return;
        }
        let was_shown = self.overlay_shown.flatten().is_some();
        self.overlay_shown = Some(offset);
        // SAFETY: our visuals, on the window's thread.
        unsafe {
            match offset {
                Some((x, y)) => {
                    let _ = overlay.SetOffsetX2(x as f32);
                    let _ = overlay.SetOffsetY2(y as f32);
                    if !was_shown {
                        let _ = self.overlay_root.AddVisual(overlay, false, None);
                    }
                }
                None => {
                    let _ = self.overlay_root.RemoveVisual(overlay);
                }
            }
        }
    }
    /// Move native views to `split`'s placements.
    pub fn apply(&mut self, natives: &NativeViews, split: &PlaneSplit) {
        let scale = self.scale();
        let views = natives.views();
        // Unmounted: their cuts go (pages stay with their controllers).
        self.clips.retain(|(node, clip)| {
            let kept = views.iter().any(|(other, _)| other == node);
            if !kept {
                if let Some(page) = &clip.page {
                    page.unmount(&clip.inner);
                }
                if clip.shown {
                    // SAFETY: removes our visual from our root.
                    let _ = unsafe { self.composition.root.RemoveVisual(clip.outer()) };
                }
            }
            kept
        });
        self.order
            .retain(|node| views.iter().any(|(other, _)| other == node));
        for (node, content) in &views {
            if !self.clips.iter().any(|(other, _)| other == node) {
                match self.clip() {
                    Some(clip) => self.clips.push((*node, clip)),
                    None => continue,
                }
            }
            let clip = &mut self
                .clips
                .iter_mut()
                .find(|(other, _)| other == node)
                .unwrap()
                .1;
            // Mount what it shows (a page is made on first mount).
            let (page, web) = match content {
                // A closed page shows nothing (and gets no new page).
                NativeContent::Web(web) if web.is_closed() => (None, Some(web.clone())),
                NativeContent::Web(web) => {
                    (webview2::backend(web, self.hwnd, scale), Some(web.clone()))
                }
                _ => (None, None),
            };
            let same = match (&clip.page, &page) {
                (Some(a), Some(b)) => Rc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            };
            if !same {
                if let Some(old) = &clip.page {
                    old.unmount(&clip.inner);
                }
                if let Some(page) = &page {
                    let composition = Rc::downgrade(&self.composition);
                    page.mount(
                        &clip.inner,
                        self.hwnd,
                        Rc::new(move || {
                            if let Some(composition) = composition.upgrade() {
                                composition.commit();
                            }
                        }),
                    );
                }
                clip.page = page;
                clip.placed = None;
            }
            clip.web = web;
            Self::place(&self.composition, clip, split.native(*node).cloned(), scale);
        }
        // Shown pages in paint order, between the base and the overlay.
        let mut shown: Vec<(usize, NodeId)> = self
            .clips
            .iter()
            .filter(|(_, clip)| clip.shown)
            .map(|(node, clip)| {
                (
                    clip.placed.as_ref().map_or(usize::MAX, |p| p.0.order),
                    *node,
                )
            })
            .collect();
        shown.sort_by_key(|(order, _)| *order);
        let wanted: Vec<NodeId> = shown.into_iter().map(|(_, node)| node).collect();
        if wanted != self.order {
            let root = &self.composition.root;
            for (node, clip) in &self.clips {
                if self.order.contains(node) {
                    // SAFETY: removes our visual from our root.
                    let _ = unsafe { root.RemoveVisual(clip.outer()) };
                }
            }
            for node in &wanted {
                if let Some((_, clip)) = self.clips.iter().find(|(other, _)| other == node) {
                    // Each under the overlay: the last added is the top page.
                    // SAFETY: adds our visual to our root.
                    let _ = unsafe { root.AddVisual(clip.outer(), false, &self.overlay_root) };
                }
            }
            self.order = wanted;
        }
    }
    /// Nested cuts and the visual holding the page.
    fn clip(&self) -> Option<Clip> {
        let device = &self.composition.device;
        // SAFETY: visuals of our device, nested here.
        unsafe {
            let mut cuts: Vec<Cut> = Vec::with_capacity(NATIVE_CUTS);
            for _ in 0..NATIVE_CUTS {
                let visual = device.CreateVisual().ok()?;
                // Rounded clips antialiased.
                let _ = visual.SetBorderMode(DCOMPOSITION_BORDER_MODE_SOFT);
                if let Some(parent) = cuts.last() {
                    parent.visual.AddVisual(&visual, false, None).ok()?;
                }
                cuts.push(Cut {
                    visual,
                    clip: None,
                    shape: None,
                });
            }
            let inner = device.CreateVisual().ok()?;
            cuts.last()?.visual.AddVisual(&inner, false, None).ok()?;
            let effect = device.CreateEffectGroup().ok();
            if let Some(effect) = &effect {
                let _ = cuts[0].visual.SetEffect(effect);
            }
            Some(Clip {
                cuts,
                effect,
                opacity: 1.,
                inner,
                web: None,
                page: None,
                placed: None,
                shown: false,
            })
        }
    }
    fn place(
        composition: &Composition,
        clip: &mut Clip,
        placement: Option<NativePlacement>,
        scale: f32,
    ) {
        let visible = placement
            .as_ref()
            .and_then(|p| p.visible())
            .filter(|_| clip.page.is_some());
        let (Some(placement), Some(visible)) = (placement, visible) else {
            if clip.shown {
                // Hidden: keyboard focus leaves it.
                if let Some(web) = &clip.web
                    && web.focused()
                {
                    web.blur();
                }
                if let Some(page) = &clip.page {
                    page.set_visible(false);
                }
                clip.shown = false;
            }
            clip.placed = None;
            return;
        };
        let key = (placement.clone(), scale);
        if clip.placed.as_ref() != Some(&key) {
            let frame = frame_px(placement.frame, scale);
            // SAFETY: our visual, on the window's thread.
            unsafe {
                let _ = clip.inner.SetOffsetX2(frame.0 as f32);
                let _ = clip.inner.SetOffsetY2(frame.1 as f32);
            }
            if let Some(page) = &clip.page {
                page.set_bounds(frame, scale);
            }
            if clip.opacity != placement.opacity
                && let Some(effect) = &clip.effect
            {
                // SAFETY: our effect group.
                let _ = unsafe { effect.SetOpacity2(placement.opacity.clamp(0., 1.)) };
                clip.opacity = placement.opacity;
            }
        }
        let shapes = shapes(visible, &placement.rounded, clip.cuts.len());
        for (cut, shape) in clip.cuts.iter_mut().zip(shapes) {
            cut.cut(&composition.device, shape, scale);
        }
        clip.placed = Some(key);
        if !clip.shown {
            if let Some(page) = &clip.page {
                page.set_visible(true);
            }
            clip.shown = true;
        }
    }
    /// Agree zgx's focus with the pages' (see `NativeHost::sync_focus`).
    pub fn sync_focus(&mut self, ui: &Ui, natives: &NativeViews) {
        let focused = self.clips.iter().find_map(|(node, clip)| {
            clip.page
                .as_ref()
                .filter(|page| page.focused())
                .map(|_| *node)
        });
        if focused == self.focus {
            return;
        }
        let previous = std::mem::replace(&mut self.focus, focused);
        for (node, clip) in &self.clips {
            let has = focused == Some(*node);
            if let Some(web) = &clip.web
                && web.focused() != has
            {
                web.update_state(|state| state.focused = has);
            }
        }
        match focused {
            // A click in the page, or the page focusing itself.
            Some(node) if ui.input.focused() != Some(node) && natives.contains(node) => {
                ui.input.focus(&ui.scene, Some(node));
            }
            // Focus moved away (a click on zgx content).
            None if previous.is_some() && ui.input.focused() == previous => {
                ui.input.focus(&ui.scene, None);
            }
            _ => {}
        }
    }
}
impl Drop for Surfaces {
    fn drop(&mut self) {
        for (_, clip) in &self.clips {
            if let Some(page) = &clip.page {
                page.unmount(&clip.inner);
            }
        }
        // SAFETY: removes our subclass if the window still has it (once it
        // is destroyed, `attached` is false and its handle may be another
        // window's), and only once removed takes back the reference
        // `Surfaces::new` leaked for it (`Rc::as_ptr` is that pointer); then
        // our visuals from our tree.
        unsafe {
            if self.pointer.attached.replace(false)
                && RemoveWindowSubclass(self.hwnd, Some(subclass), SUBCLASS_ID).as_bool()
            {
                drop(Rc::from_raw(Rc::as_ptr(&self.pointer)));
            }
            for (_, clip) in &self.clips {
                if clip.shown {
                    let _ = self.composition.root.RemoveVisual(clip.outer());
                }
            }
            let _ = self.composition.root.RemoveVisual(&self.overlay_root);
        }
        self.composition.commit();
    }
}
