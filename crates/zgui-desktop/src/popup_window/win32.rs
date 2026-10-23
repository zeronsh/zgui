//! Popup windows on Windows: owned, never-activated tool windows, each drawn
//! through a DirectComposition visual of its own (see the module's notes).
//!
//! A popup's window is `WS_POPUP` with `WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW
//! | WS_EX_NOREDIRECTIONBITMAP`, owned by the window it belongs to (above it,
//! hidden and minimized with it, never on the taskbar). It answers
//! `WM_MOUSEACTIVATE` with `MA_NOACTIVATE`, so the parent keeps the keyboard
//! and keys reach the popup's content through the window's focus. Its pointer
//! input goes to the parent's host, in the parent's coordinates. Its frames
//! are an overlay renderer's (`GpuRenderer::for_overlay_visual`) drawing the
//! popup root alone, in a swap chain with alpha. A menu or popover that is
//! one rounded panel gets DWM's rounded corners, rim and shadow, as Windows
//! 11's menus do (`corner_of`).
//!
//! The parent window is subclassed while it has popups: an owned window does
//! not follow its owner, so popups move with it (`WM_WINDOWPOSCHANGED`), and
//! close as the app deactivates (`WM_ACTIVATEAPP`) and on a press in its
//! frame or title bar (`WM_NCLBUTTONDOWN`, which zgui never hears).
//!
//! Windows are pooled per thread and reused hidden, made only once the
//! pointer reaches something that opens a popup (`warm`): an app that never
//! opens one makes none. Without Direct3D 12 (the Vulkan fallback) there is
//! no visual to draw into, and popups stay inside their window.
use super::PopupInput;
use std::{
    cell::{Cell, RefCell},
    mem::ManuallyDrop,
    rc::Rc,
};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
        Graphics::{
            DirectComposition::{
                DCompositionCreateDevice2, IDCompositionDevice, IDCompositionTarget,
                IDCompositionVisual,
            },
            Dwm::{
                DWM_WINDOW_CORNER_PREFERENCE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND,
                DWMWCP_ROUND, DWMWCP_ROUNDSMALL, DwmSetWindowAttribute,
            },
            Gdi::{
                ClientToScreen, EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO,
                ScreenToClient,
            },
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            HiDpi::GetDpiForWindow,
            Input::KeyboardAndMouse::{
                EnableWindow, GetCapture, ReleaseCapture, SetCapture, TME_LEAVE, TRACKMOUSEEVENT,
                TrackMouseEvent,
            },
            Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
            WindowsAndMessaging::{
                CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow, GA_ROOT,
                GW_HWNDNEXT, GWLP_HWNDPARENT, GWLP_USERDATA, GetAncestor, GetClientRect,
                GetCursorPos, GetWindow, GetWindowLongPtrW, GetWindowRect, HTCLIENT,
                HWND_NOTOPMOST, HWND_TOP, HWND_TOPMOST, IDC_ARROW, IsWindowVisible, KillTimer,
                LoadCursorW, MA_NOACTIVATE, RegisterClassExW, SPI_SETWORKAREA, SW_HIDE,
                SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE,
                SWP_NOZORDER, SWP_SHOWWINDOW, SetCursor, SetTimer, SetWindowLongPtrW, SetWindowPos,
                ShowWindow, WINDOWPOS, WM_ACTIVATEAPP, WM_DISPLAYCHANGE, WM_LBUTTONDBLCLK,
                WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDBLCLK, WM_MBUTTONDOWN, WM_MBUTTONUP,
                WM_MOUSEACTIVATE, WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCDESTROY,
                WM_NCHITTEST, WM_NCLBUTTONDOWN, WM_NCMBUTTONDOWN, WM_NCRBUTTONDOWN,
                WM_RBUTTONDBLCLK, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SETCURSOR, WM_SETTINGCHANGE,
                WM_TIMER, WM_WINDOWPOSCHANGED, WM_XBUTTONDBLCLK, WM_XBUTTONDOWN, WM_XBUTTONUP,
                WNDCLASSEXW, WS_EX_NOACTIVATE, WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOOLWINDOW,
                WS_POPUP, WindowFromPoint,
            },
        },
    },
    core::{BOOL, w},
};
use winit::event::{MouseButton, MouseScrollDelta, TouchPhase};
use zgui::{
    scene::{NodeId, NodeKind, Rect, Scene},
    semantics::Semantics,
};
use zgui_gpu::{GpuContext, GpuError, GpuRenderer};

/// The pointer left the window (TrackMouseEvent's; not in the bindings' module).
const WM_MOUSELEAVE: u32 = 0x02a3;
const SUBCLASS_ID: usize = 0x7a67_7570; // "zgup"
/// A mouse message's buttons still down (`MK_LBUTTON | MK_RBUTTON |
/// MK_MBUTTON | MK_XBUTTON1 | MK_XBUTTON2`).
const BUTTONS: usize = 0x01 | 0x02 | 0x10 | 0x20 | 0x40;

/// How many windows stay made while hidden: a menu, its submenu and a
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

/// Where a popup window's input goes: its popup's origin in its parent
/// window (logical pixels), the scale it draws at, and the parent's host.
#[derive(Default)]
struct Target {
    origin: Cell<(f32, f32)>,
    scale: Cell<f32>,
    send: RefCell<Option<Sink>>,
    parent: Cell<usize>,
    tracking: Cell<bool>,
}
impl Target {
    fn send(&self, input: PopupInput) {
        let send = self.send.borrow().clone();
        if let Some(send) = send {
            send(input);
        }
    }
    /// Client point `(x, y)` (physical pixels) in the parent's coordinates.
    fn point(&self, (x, y): (i32, i32)) -> (f32, f32) {
        let (left, top) = self.origin.get();
        let scale = self.scale.get().max(0.5);
        (left + x as f32 / scale, top + y as f32 / scale)
    }
}

fn client_point(lparam: LPARAM) -> (i32, i32) {
    (
        (lparam.0 & 0xffff) as i16 as i32,
        ((lparam.0 >> 16) & 0xffff) as i16 as i32,
    )
}

unsafe extern "system" fn popup_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_TIMER if wparam.0 == SLIDE_TIMER => {
            slide_step(hwnd);
            return LRESULT(0);
        }
        // Clicks act without taking the keyboard from the parent.
        WM_MOUSEACTIVATE => return LRESULT(MA_NOACTIVATE as isize),
        WM_NCHITTEST => return LRESULT(HTCLIENT as isize),
        WM_SETCURSOR => {
            // SAFETY: a system cursor, set while over our window.
            unsafe {
                if let Ok(cursor) = LoadCursorW(None, IDC_ARROW) {
                    SetCursor(Some(cursor));
                }
            }
            return LRESULT(1);
        }
        _ => {}
    }
    // SAFETY: our window's slot holds its `Target` (`Panel::new`), alive
    // until the window is destroyed (`Panel::drop` clears it first).
    let target = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const Target;
    if let Some(target) = unsafe { target.as_ref() }
        && pointer(hwnd, target, message, wparam, lparam)
    {
        return LRESULT(0);
    }
    // SAFETY: the default procedure, for our own window.
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

/// A popup window's pointer message, sent to its parent's host: whether it
/// was one.
fn pointer(hwnd: HWND, target: &Target, message: u32, wparam: WPARAM, lparam: LPARAM) -> bool {
    let button = |button: MouseButton, pressed: bool| {
        let (x, y) = target.point(client_point(lparam));
        // A press holds the pointer, as a window's does: dragged out of the
        // popup, its moves still come here.
        // SAFETY: capture for our own window, on its thread.
        unsafe {
            if pressed {
                SetCapture(hwnd);
            } else if wparam.0 & BUTTONS == 0 && GetCapture() == hwnd {
                let _ = ReleaseCapture();
            }
        }
        target.send(PopupInput::Button {
            x,
            y,
            button,
            pressed,
        });
    };
    let extra = || match (wparam.0 >> 16) & 0xffff {
        1 => MouseButton::Back,
        _ => MouseButton::Forward,
    };
    match message {
        WM_MOUSEMOVE => {
            if !target.tracking.replace(true) {
                let mut track = TRACKMOUSEEVENT {
                    cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                // SAFETY: tracks our own window.
                let _ = unsafe { TrackMouseEvent(&mut track) };
            }
            let (x, y) = target.point(client_point(lparam));
            target.send(PopupInput::Moved { x, y });
        }
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => button(MouseButton::Left, true),
        WM_LBUTTONUP => button(MouseButton::Left, false),
        WM_RBUTTONDOWN | WM_RBUTTONDBLCLK => button(MouseButton::Right, true),
        WM_RBUTTONUP => button(MouseButton::Right, false),
        WM_MBUTTONDOWN | WM_MBUTTONDBLCLK => button(MouseButton::Middle, true),
        WM_MBUTTONUP => button(MouseButton::Middle, false),
        WM_XBUTTONDOWN | WM_XBUTTONDBLCLK => button(extra(), true),
        WM_XBUTTONUP => button(extra(), false),
        WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
            // A wheel's point is on the screen.
            let (x, y) = client_point(lparam);
            let mut point = POINT { x, y };
            // SAFETY: converts into our own window's client.
            let _ = unsafe { ScreenToClient(hwnd, &mut point) };
            let (x, y) = target.point((point.x, point.y));
            // As winit reports the window's own.
            let notches = ((wparam.0 >> 16) & 0xffff) as i16 as f32 / 120.;
            let delta = if message == WM_MOUSEWHEEL {
                MouseScrollDelta::LineDelta(0., notches)
            } else {
                MouseScrollDelta::LineDelta(-notches, 0.)
            };
            target.send(PopupInput::Wheel {
                x,
                y,
                delta,
                phase: TouchPhase::Moved,
            });
        }
        WM_MOUSELEAVE => {
            target.tracking.set(false);
            // Into the parent window, its own messages take over (a leave
            // sent after them would clear what they hover).
            let parent = HWND(target.parent.get() as *mut _);
            if !parent.is_invalid() && over_client(parent) {
                return true;
            }
            target.send(PopupInput::Left);
        }
        _ => return false,
    }
    true
}

/// Whether the pointer is over `hwnd`'s client area.
fn over_client(hwnd: HWND) -> bool {
    let mut point = POINT::default();
    let mut client = RECT::default();
    // SAFETY: reads the pointer and a window of ours into locals.
    unsafe {
        if GetCursorPos(&mut point).is_err()
            || !ScreenToClient(hwnd, &mut point).as_bool()
            || GetClientRect(hwnd, &mut client).is_err()
        {
            return false;
        }
    }
    point.x >= client.left
        && point.x < client.right
        && point.y >= client.top
        && point.y < client.bottom
}

thread_local! {
    /// DirectComposition's device for every popup window of the thread,
    /// left to the system as the process exits (see `POOL`).
    static DEVICE: ManuallyDrop<RefCell<Option<IDCompositionDevice>>> =
        const { ManuallyDrop::new(RefCell::new(None)) };
}
fn device() -> Result<IDCompositionDevice, String> {
    DEVICE.with(|device| {
        if let Some(device) = &*device.borrow() {
            return Ok(device.clone());
        }
        // SAFETY: a device of our own, used on this thread.
        let made: IDCompositionDevice =
            unsafe { DCompositionCreateDevice2(None) }.map_err(|e| e.to_string())?;
        *device.borrow_mut() = Some(made.clone());
        Ok(made)
    })
}

fn class() -> windows::core::PCWSTR {
    thread_local! {
        static REGISTERED: Cell<bool> = const { Cell::new(false) };
    }
    let name = w!("ZguiPopup");
    if !REGISTERED.replace(true) {
        // SAFETY: registers a class of this module's procedure.
        unsafe {
            let instance = GetModuleHandleW(None).unwrap_or_default();
            let class = WNDCLASSEXW {
                cbSize: size_of::<WNDCLASSEXW>() as u32,
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(popup_proc),
                hInstance: instance.into(),
                lpszClassName: name,
                ..Default::default()
            };
            RegisterClassExW(&class);
        }
    }
    name
}

/// A popup window: its composition and the renderer drawing into it.
struct Panel {
    hwnd: HWND,
    target: Rc<Target>,
    _composition: IDCompositionTarget,
    visual: IDCompositionVisual,
    device: IDCompositionDevice,
    renderer: Option<GpuRenderer>,
    /// The scale its renderer was made for.
    scale: f32,
    /// Its box in its parent window's coordinates, as last placed.
    rect: Option<Rect>,
    /// Where it is on the screen (physical pixels), as last placed.
    screen: Option<(i32, i32, i32, i32)>,
    /// Its popup root's layer revision it last drew.
    revision: Option<u64>,
    /// A drag preview's: its box's offset from where the scene lays its root
    /// out (its window goes where the drag puts the root).
    preview: Option<(f32, f32)>,
    /// How DWM rounds it, as last set.
    corner: Option<DWM_WINDOW_CORNER_PREFERENCE>,
}

/// How DWM frames popup root `root`'s window at `rect`: as Windows 11 frames
/// its menus, with rounded corners, a rim and a shadow, when what it shows is
/// one rounded panel filling it; otherwise not at all, zgui drawing all of it
/// (DWM's shadow would double one of zgui's, past the panel).
///
/// Not with a menu's acrylic: DWM draws a never-activated window's system
/// backdrop as a flat colour, and a popup must not take activation (its
/// window keeps the keyboard).
fn corner_of(scene: &Scene, root: NodeId, rect: Rect) -> DWM_WINDOW_CORNER_PREFERENCE {
    let Some(&panel) = scene.children(root).first() else {
        return DWMWCP_DONOTROUND;
    };
    let (NodeKind::Quad(quad) | NodeKind::Panel { quad, .. }) = scene.kind(panel) else {
        return DWMWCP_DONOTROUND;
    };
    let bounds = scene.bounds(panel);
    if (bounds.width - rect.width).abs() > 1. || (bounds.height - rect.height).abs() > 1. {
        return DWMWCP_DONOTROUND;
    }
    // DWM's two radii: 8 and 4 pixels at 100%.
    match quad.radius {
        r if r >= 6. => DWMWCP_ROUND,
        r if r > 0. => DWMWCP_ROUNDSMALL,
        _ => DWMWCP_DONOTROUND,
    }
}
impl Panel {
    fn new() -> Result<Self, String> {
        let device = device()?;
        // SAFETY: a window of our class and the composition of it, on this
        // thread; its target slot is cleared before the window goes.
        unsafe {
            let hwnd = CreateWindowExW(
                WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_NOREDIRECTIONBITMAP,
                class(),
                w!(""),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                Some(GetModuleHandleW(None).unwrap_or_default().into()),
                None,
            )
            .map_err(|e| e.to_string())?;
            let target = Rc::new(Target::default());
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, Rc::as_ptr(&target) as isize);
            let made = (|| {
                let composition = device.CreateTargetForHwnd(hwnd, true)?;
                let visual = device.CreateVisual()?;
                composition.SetRoot(&visual)?;
                Ok::<_, windows::core::Error>((composition, visual))
            })();
            let (composition, visual) = match made {
                Ok(made) => made,
                Err(error) => {
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                    let _ = DestroyWindow(hwnd);
                    return Err(error.to_string());
                }
            };
            Ok(Self {
                hwnd,
                target,
                _composition: composition,
                visual,
                device,
                renderer: None,
                scale: 0.,
                rect: None,
                screen: None,
                revision: None,
                preview: None,
                corner: None,
            })
        }
    }
    /// Have DWM round it as `corner` (`corner_of`).
    fn set_corner(&mut self, corner: DWM_WINDOW_CORNER_PREFERENCE) {
        if self.corner.replace(corner) == Some(corner) {
            return;
        }
        // SAFETY: an attribute of our own window, from a local of its type.
        let _ = unsafe {
            DwmSetWindowAttribute(
                self.hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                &corner as *const _ as *const _,
                size_of_val(&corner) as u32,
            )
        };
    }
    /// Its renderer, made for `scale` (again for a new one).
    fn renderer(&mut self, context: &GpuContext, scale: f32) -> Result<&mut GpuRenderer, GpuError> {
        if self.scale != scale {
            self.renderer = None;
        }
        if self.renderer.is_none() {
            self.scale = scale;
            self.renderer = Some(GpuRenderer::for_overlay_visual(
                self.visual.clone(),
                self.device.clone(),
                context,
                scale,
            )?);
        }
        Ok(self.renderer.as_mut().expect("made above"))
    }
    /// Place it at `rect` (its parent's coordinates), its parent's client
    /// origin on the screen at `origin`.
    fn place(&mut self, origin: (i32, i32), rect: Rect) {
        self.rect = Some(rect);
        let screen = screen_rect(origin, self.scale, rect);
        self.target.origin.set((rect.x, rect.y));
        self.target.scale.set(self.scale);
        if self.screen == Some(screen) {
            return;
        }
        self.screen = Some(screen);
        // SAFETY: moves our own window, never activating it.
        let _ = unsafe {
            SetWindowPos(
                self.hwnd,
                None,
                screen.0,
                screen.1,
                screen.2,
                screen.3,
                SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOOWNERZORDER,
            )
        };
    }
    /// Show it over `parent`, which owns it from now on. A drag preview
    /// shows over every window, and disabled: it takes no input, and the
    /// window under a point is the one past it (`window_at`).
    fn show(&self, parent: HWND, send: &Sink) {
        *self.target.send.borrow_mut() = Some(send.clone());
        self.target.parent.set(parent.0 as usize);
        let preview = self.preview.is_some();
        if preview {
            PREVIEWS.with(|previews| previews.borrow_mut().push(self.hwnd.0 as isize));
        }
        // SAFETY: our window; its owner is a live window of this thread.
        unsafe {
            SetWindowLongPtrW(self.hwnd, GWLP_HWNDPARENT, parent.0 as isize);
            let _ = EnableWindow(self.hwnd, !preview);
            let _ = SetWindowPos(
                self.hwnd,
                Some(if preview { HWND_TOPMOST } else { HWND_TOP }),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        }
    }
    fn hide(&mut self) {
        // SAFETY: our own window.
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
            if GetCapture() == self.hwnd {
                let _ = ReleaseCapture();
            }
            SetWindowLongPtrW(self.hwnd, GWLP_HWNDPARENT, 0);
            if self.preview.take().is_some() {
                let hwnd = self.hwnd.0 as isize;
                PREVIEWS.with(|previews| previews.borrow_mut().retain(|preview| *preview != hwnd));
                let _ = EnableWindow(self.hwnd, true);
                let _ = SetWindowPos(
                    self.hwnd,
                    Some(HWND_NOTOPMOST),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
            }
        }
        *self.target.send.borrow_mut() = None;
        self.target.parent.set(0);
        self.target.tracking.set(false);
        self.rect = None;
        self.screen = None;
        self.revision = None;
        if let Some(renderer) = &mut self.renderer {
            renderer.trim();
        }
    }
}
impl Drop for Panel {
    fn drop(&mut self) {
        self.renderer = None;
        // SAFETY: our own window; its slot is cleared before it goes.
        unsafe {
            SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

thread_local! {
    /// Hidden windows, ready to show; released as the last window's host
    /// goes, while the GPU device is alive (released in the thread's
    /// teardown, after it, they ended the process partway).
    static POOL: RefCell<Vec<Panel>> = const { RefCell::new(Vec::new()) };
    /// Live `PopupHost`s, one per window.
    static HOSTS: Cell<usize> = const { Cell::new(0) };
    /// Drag previews' windows, which `window_at` looks past.
    static PREVIEWS: RefCell<Vec<isize>> = const { RefCell::new(Vec::new()) };
    /// Windows that have shown popups, their hosts, client origins and
    /// shown popup windows.
    static PARENTS: RefCell<Vec<Parent>> = const { RefCell::new(Vec::new()) };
    /// No popup window could be made (no Direct3D 12): popups stay inside
    /// their windows from then on.
    static UNSUPPORTED: Cell<bool> = const { Cell::new(false) };
}
struct Parent {
    hwnd: usize,
    send: Sink,
    origin: (i32, i32),
    popups: Vec<usize>,
}

fn acquire() -> Result<Panel, String> {
    match POOL.with(|pool| pool.borrow_mut().pop()) {
        Some(panel) => Ok(panel),
        None => Panel::new(),
    }
}
fn release(mut panel: Panel) {
    panel.hide();
    POOL.with(|pool| {
        let mut pool = pool.borrow_mut();
        if pool.len() < pool_size() {
            pool.push(panel);
        }
    });
}

/// How long a drag preview nothing took slides back to its source, as
/// AppKit's drag images do.
const SLIDE: std::time::Duration = std::time::Duration::from_millis(200);
/// The timer stepping a slide (`WM_TIMER`'s id on the preview's window).
const SLIDE_TIMER: usize = 0x7a67;
/// A drag preview's window sliding back: from and to (screen pixels), since.
struct Slide {
    panel: Panel,
    from: (i32, i32),
    to: (i32, i32),
    began: std::time::Instant,
}
thread_local! {
    static SLIDING: RefCell<Vec<Slide>> = const { RefCell::new(Vec::new()) };
}
/// Slide `panel`'s window to `to` (screen pixels, its top-left), easing
/// out, then put it back in the pool.
fn slide_back(panel: Panel, to: (i32, i32)) {
    let Some((x, y, ..)) = panel.screen else {
        release(panel);
        return;
    };
    let hwnd = panel.hwnd;
    SLIDING.with(|sliding| {
        sliding.borrow_mut().push(Slide {
            panel,
            from: (x, y),
            to,
            began: std::time::Instant::now(),
        })
    });
    // SAFETY: a timer on our own window, stepped in `popup_proc`.
    unsafe { SetTimer(Some(hwnd), SLIDE_TIMER, 8, None) };
}
/// A step of `hwnd`'s slide: moved along, or, done, back in the pool.
fn slide_step(hwnd: HWND) {
    let done = SLIDING.with(|sliding| {
        let mut sliding = sliding.borrow_mut();
        let index = sliding.iter().position(|slide| slide.panel.hwnd == hwnd)?;
        let slide = &sliding[index];
        let t = (slide.began.elapsed().as_secs_f32() / SLIDE.as_secs_f32()).min(1.);
        let eased = 1. - (1. - t) * (1. - t);
        let at = |from: i32, to: i32| from + ((to - from) as f32 * eased).round() as i32;
        // SAFETY: moves our own window, never activating it.
        let _ = unsafe {
            SetWindowPos(
                hwnd,
                None,
                at(slide.from.0, slide.to.0),
                at(slide.from.1, slide.to.1),
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOOWNERZORDER | SWP_NOACTIVATE,
            )
        };
        (t >= 1.).then(|| sliding.swap_remove(index).panel)
    });
    if let Some(panel) = done {
        // SAFETY: our own window's timer.
        let _ = unsafe { KillTimer(Some(hwnd), SLIDE_TIMER) };
        release(panel);
    }
}

/// Have a window and its renderer ready for the popup about to open (the
/// pointer reached something that opens one, `Ui::on_popup_soon`), if none
/// is. It is shown once, empty and off screen, then hidden: a window's
/// first showing is what costs.
pub(crate) fn warm(context: &GpuContext, scale: f32) {
    if !enabled() || POOL.with(|pool| !pool.borrow().is_empty()) {
        return;
    }
    let Ok(mut panel) = Panel::new() else {
        UNSUPPORTED.set(true);
        return;
    };
    if panel.renderer(context, scale).is_err() {
        UNSUPPORTED.set(true);
        return;
    }
    // SAFETY: our own window, shown without activating, then hidden.
    unsafe {
        let _ = SetWindowPos(
            panel.hwnd,
            None,
            -32_000,
            -32_000,
            1,
            1,
            SWP_NOACTIVATE | SWP_NOZORDER | SWP_SHOWWINDOW,
        );
        let _ = ShowWindow(panel.hwnd, SW_HIDE);
    }
    POOL.with(|pool| pool.borrow_mut().push(panel));
}

/// Whether popups show in windows of their own: but with
/// `ZGUI_POPUP_WINDOWS=0` (inside their window, as elsewhere), or once none
/// could be made.
pub(crate) fn enabled() -> bool {
    thread_local! {
        static ON: bool = std::env::var_os("ZGUI_POPUP_WINDOWS").is_none_or(|v| v != "0");
    }
    ON.with(|on| *on) && !UNSUPPORTED.get()
}

fn hwnd_of(window: &winit::window::Window) -> Option<HWND> {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(HWND(handle.hwnd.get() as *mut _)),
        _ => None,
    }
}
/// The window's client origin on the screen in global coordinates: the
/// screen's pixels, the same for windows of every scale (`global_scale`), as
/// a platform drag reports them.
pub(crate) fn content_origin(window: &winit::window::Window) -> Option<(f64, f64)> {
    let (x, y) = client_origin(hwnd_of(window)?);
    Some((f64::from(x), f64::from(y)))
}
/// The window's handle, as `window_at` names windows.
pub(crate) fn window_number(window: &winit::window::Window) -> Option<isize> {
    hwnd_of(window).map(|hwnd| hwnd.0 as isize)
}
/// The front-most window (any app's) at global point `(x, y)` (the screen's
/// pixels, `content_origin`), past drag previews (`PREVIEWS`: below one, the
/// next window down there).
pub(crate) fn window_at(x: f64, y: f64) -> Option<isize> {
    // SAFETY: reads the system's windows; the window at a point and its
    // top-level ancestor into locals.
    unsafe {
        let point = POINT {
            x: x.round() as i32,
            y: y.round() as i32,
        };
        let window = WindowFromPoint(point);
        if window.is_invalid() {
            return None;
        }
        let root = GetAncestor(window, GA_ROOT);
        let mut window = if root.is_invalid() { window } else { root };
        // A preview: the next visible window down holding the point.
        while PREVIEWS.with(|previews| previews.borrow().contains(&(window.0 as isize))) {
            window = GetWindow(window, GW_HWNDNEXT).ok()?;
            while !holds(window, point) {
                window = GetWindow(window, GW_HWNDNEXT).ok()?;
            }
        }
        Some(window.0 as isize)
    }
}
/// Whether top-level `window` shows at screen point `point`.
fn holds(window: HWND, point: POINT) -> bool {
    let mut rect = RECT::default();
    // SAFETY: reads a window's visibility and rectangle into a local.
    unsafe {
        IsWindowVisible(window).as_bool()
            && GetWindowRect(window, &mut rect).is_ok()
            && (rect.left..rect.right).contains(&point.x)
            && (rect.top..rect.bottom).contains(&point.y)
    }
}
fn client_origin(hwnd: HWND) -> (i32, i32) {
    let mut origin = POINT::default();
    // SAFETY: converts into a local, for our own window.
    let _ = unsafe { ClientToScreen(hwnd, &mut origin) };
    (origin.x, origin.y)
}
fn window_scale(hwnd: HWND) -> f32 {
    // SAFETY: reads our own window's DPI.
    unsafe { GetDpiForWindow(hwnd) }.max(96) as f32 / 96.
}

/// Where popups may show: each monitor's work area (less the taskbar), in
/// the window's coordinates. None when popups stay inside their window.
pub(crate) fn popup_screens(window: &winit::window::Window) -> Vec<Rect> {
    if !enabled() {
        return Vec::new();
    }
    let Some(hwnd) = hwnd_of(window) else {
        return Vec::new();
    };
    screens_in(client_origin(hwnd), window_scale(hwnd), &work_areas())
}

/// Monitors' work `areas` (physical pixels) in the coordinates of a window
/// whose client origin is at `origin`, at its `scale`.
///
/// A popup draws at its window's scale on any monitor, as the window's own
/// pixels do on a monitor of another scale it reaches onto: zgui keeps a
/// popup on the screen its trigger is on, so it lies on another monitor than
/// its window only beside a trigger drawn there at the window's scale. One
/// scale for both keeps it the size of its trigger, beside it, and inside
/// the area these give.
fn screens_in(origin: (i32, i32), scale: f32, areas: &[RECT]) -> Vec<Rect> {
    areas
        .iter()
        .map(|area| {
            Rect::new(
                (area.left - origin.0) as f32 / scale,
                (area.top - origin.1) as f32 / scale,
                (area.right - area.left) as f32 / scale,
                (area.bottom - area.top) as f32 / scale,
            )
        })
        .collect()
}
/// Where `rect` (a window's coordinates, its client origin at `origin`, at
/// its `scale`) is on the screen: left, top, width and height in physical
/// pixels, its edges rounded as the renderer snaps a popup's box.
fn screen_rect(origin: (i32, i32), scale: f32, rect: Rect) -> (i32, i32, i32, i32) {
    let left = (rect.x * scale).round() as i32;
    let top = (rect.y * scale).round() as i32;
    let right = ((rect.x + rect.width) * scale).round() as i32;
    let bottom = ((rect.y + rect.height) * scale).round() as i32;
    (
        origin.0 + left,
        origin.1 + top,
        (right - left).max(1),
        (bottom - top).max(1),
    )
}
fn work_areas() -> Vec<RECT> {
    unsafe extern "system" fn each(monitor: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        // SAFETY: `data` is the vector `work_areas` passes, alive for the
        // enumeration; the monitor is the system's.
        unsafe {
            if GetMonitorInfoW(monitor, &mut info).as_bool() {
                (*(data.0 as *mut Vec<RECT>)).push(info.rcWork);
            }
        }
        true.into()
    }
    let mut areas: Vec<RECT> = Vec::new();
    // SAFETY: the callback writes only into `areas`, during this call.
    let _ = unsafe {
        EnumDisplayMonitors(
            None,
            None,
            Some(each),
            LPARAM(&mut areas as *mut _ as isize),
        )
    };
    areas
}
/// The parent's subclass: popups follow it, and close as the app
/// deactivates or its frame is pressed.
unsafe extern "system" fn parent_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    let send = |input: PopupInput| {
        let send = PARENTS.with(|parents| {
            let parents = parents.borrow();
            let parent = parents
                .iter()
                .find(|parent| parent.hwnd == hwnd.0 as usize)?;
            (!parent.popups.is_empty() || matches!(input, PopupInput::Screens))
                .then(|| parent.send.clone())
        });
        if let Some(send) = send {
            send(input);
        }
    };
    match message {
        WM_ACTIVATEAPP if wparam.0 == 0 => send(PopupInput::Dismiss),
        WM_NCLBUTTONDOWN | WM_NCRBUTTONDOWN | WM_NCMBUTTONDOWN => send(PopupInput::Dismiss),
        WM_DISPLAYCHANGE => send(PopupInput::Screens),
        WM_SETTINGCHANGE if wparam.0 as u32 == SPI_SETWORKAREA.0 => send(PopupInput::Screens),
        WM_WINDOWPOSCHANGED => {
            // SAFETY: the system's WINDOWPOS for this message.
            let flags = unsafe { (*(lparam.0 as *const WINDOWPOS)).flags };
            if !flags.contains(SWP_NOMOVE) {
                follow(hwnd);
            }
        }
        WM_NCDESTROY => {
            // SAFETY: removes this subclass from the window going away.
            let _ = unsafe { RemoveWindowSubclass(hwnd, Some(parent_proc), SUBCLASS_ID) };
        }
        _ => {}
    }
    // SAFETY: the next procedure in the window's chain.
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}
/// Move `parent`'s popup windows as far as it moved.
fn follow(parent: HWND) {
    let origin = client_origin(parent);
    let moved = PARENTS.with(|parents| {
        let mut parents = parents.borrow_mut();
        let entry = parents
            .iter_mut()
            .find(|entry| entry.hwnd == parent.0 as usize)?;
        let delta = (origin.0 - entry.origin.0, origin.1 - entry.origin.1);
        entry.origin = origin;
        (delta != (0, 0)).then(|| (delta, entry.popups.clone()))
    });
    let Some(((dx, dy), popups)) = moved else {
        return;
    };
    for popup in popups {
        let hwnd = HWND(popup as *mut _);
        let mut rect = RECT::default();
        // SAFETY: moves a popup window of ours, never activating it.
        unsafe {
            if GetWindowRect(hwnd, &mut rect).is_ok() {
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    rect.left + dx,
                    rect.top + dy,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOZORDER | SWP_NOOWNERZORDER | SWP_NOACTIVATE,
                );
            }
        }
    }
}

/// A window's shown popups.
pub(crate) struct PopupHost {
    shown: Vec<(NodeId, Panel)>,
    send: Sink,
    /// The window they show over, once any has.
    parent: Option<usize>,
}
impl PopupHost {
    pub fn new(send: impl Fn(PopupInput) + 'static) -> Self {
        HOSTS.set(HOSTS.get() + 1);
        Self {
            shown: Vec::new(),
            send: Rc::new(send),
            parent: None,
        }
    }
    /// Whether any popup shows.
    pub fn showing(&self) -> bool {
        !self.shown.is_empty()
    }
    /// No popup window can be made: popups go back inside the window.
    fn unsupported(&mut self, error: impl std::fmt::Display) {
        eprintln!("zgui: popups stay inside their window: {error}");
        UNSUPPORTED.set(true);
        for (_, panel) in self.shown.drain(..) {
            drop(panel);
        }
        POOL.with(|pool| pool.borrow_mut().clear());
        (self.send)(PopupInput::Screens);
    }
    /// After the scene's flush: show each visible popup root in a window
    /// over `window`, draw it again where it changed (its layer revision,
    /// `Scene::set_popup`; always when `full`), and hide the rest.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        window: &winit::window::Window,
        scene: &Scene,
        _semantics: &Semantics,
        context: &GpuContext,
        scale: f32,
        full: bool,
        drag: Option<&zgui::input::DragState>,
        ended: Option<zgui::input::DragEnded>,
    ) -> Result<(), GpuError> {
        if self.shown.is_empty() && scene.popups().is_empty() {
            return Ok(());
        }
        let Some(parent) = hwnd_of(window) else {
            return Ok(());
        };
        let origin = client_origin(parent);
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
            // A drag nothing took: its preview slides back to its source,
            // as AppKit's drag images do. A taken one goes.
            let back = ended
                .filter(|ended| ended.preview == Some(root) && !ended.accepted)
                .filter(|ended| scene.contains(ended.source))
                .zip(panel.preview.zip(panel.rect));
            match back {
                Some((ended, ((dx, dy), rect))) => {
                    let source = scene.bounds(ended.source);
                    let to = Rect::new(source.x + dx, source.y + dy, rect.width, rect.height);
                    let (x, y, ..) = screen_rect(origin, panel.scale, to);
                    // SAFETY: shows our own window where it was let go.
                    let _ = unsafe { ShowWindow(panel.hwnd, SW_SHOWNOACTIVATE) };
                    slide_back(panel, (x, y));
                }
                None => release(panel),
            }
        }
        if !wanted.is_empty() && self.parent.replace(parent.0 as usize).is_none() {
            // SAFETY: a subclass of our own window, on its thread; it holds
            // no data of its own.
            let _ = unsafe { SetWindowSubclass(parent, Some(parent_proc), SUBCLASS_ID, 0) };
        }
        for (root, bounds) in wanted {
            let new = !self.shown.iter().any(|(shown, _)| *shown == root);
            if new {
                zgui_gpu::present_log::log(|| format!("popup show {root:?} at {bounds:?}"));
                match acquire() {
                    Ok(panel) => self.shown.push((root, panel)),
                    Err(error) => {
                        self.unsupported(error);
                        return Ok(());
                    }
                }
            }
            let panel = &mut self
                .shown
                .iter_mut()
                .find(|(shown, _)| *shown == root)
                .expect("pushed above")
                .1;
            // Popups are small: one that changed is drawn whole (and at a
            // new scale, by a new renderer).
            let revision = scene.layer_revision(root);
            let changed = new || full || panel.revision != Some(revision) || panel.scale != scale;
            panel.revision = Some(revision);
            // At its window's scale (see `screens_in`).
            let renderer = match panel.renderer(context, scale) {
                Ok(renderer) => renderer,
                Err(error) => {
                    self.unsupported(error);
                    return Ok(());
                }
            };
            renderer.set_popup(root, bounds);
            let rect = renderer.plane_rect().unwrap_or(bounds);
            let resized = panel
                .rect
                .is_none_or(|old| (old.width, old.height) != (rect.width, rect.height));
            if changed || resized {
                let renderer = panel.renderer.as_mut().expect("made above");
                renderer.repaint_all();
                renderer.render(scene, &[])?;
                renderer.present()?;
            }
            // A drag preview's window goes where the drag puts it; its frame
            // is drawn where the scene has it.
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
            panel.place(origin, frame);
            panel.set_corner(corner_of(scene, root, rect));
            if new {
                // Its first frame is in its visual before it shows.
                // SAFETY: our device, on its thread.
                let _ = unsafe { panel.device.Commit() };
                panel.show(parent, &self.send);
            }
        }
        let popups: Vec<usize> = self
            .shown
            .iter()
            .map(|(_, panel)| panel.hwnd.0 as usize)
            .collect();
        PARENTS.with(|parents| {
            let mut parents = parents.borrow_mut();
            match parents
                .iter_mut()
                .find(|entry| entry.hwnd == parent.0 as usize)
            {
                Some(entry) => {
                    entry.origin = origin;
                    entry.popups = popups;
                }
                None => parents.push(Parent {
                    hwnd: parent.0 as usize,
                    send: self.send.clone(),
                    origin,
                    popups,
                }),
            }
        });
        Ok(())
    }
    /// The drag preview's window, and its offset (`Panel::preview`).
    fn preview(&mut self, drag: &zgui::input::DragState) -> Option<&mut Panel> {
        let root = drag.preview?;
        let (_, panel) = self.shown.iter_mut().find(|(shown, _)| *shown == root)?;
        panel.preview.is_some().then_some(panel)
    }
    /// Move the drag preview's window to where `drag` puts it, drawing
    /// nothing (between frames, as the pointer moves).
    pub fn follow(&mut self, window: &winit::window::Window, drag: &zgui::input::DragState) {
        let (Some((x, y)), Some(parent)) = (drag.preview_origin, hwnd_of(window)) else {
            return;
        };
        let Some(panel) = self.preview(drag) else {
            return;
        };
        let (Some((dx, dy)), Some(rect)) = (panel.preview, panel.rect) else {
            return;
        };
        panel.place(
            client_origin(parent),
            Rect::new(x + dx, y + dy, rect.width, rect.height),
        );
    }
    /// The drag preview's last frame, as a drag image with the pointer in it
    /// (to hand the drag to Windows).
    pub fn preview_image(
        &mut self,
        drag: &zgui::input::DragState,
    ) -> Option<crate::native_drag::DragImage> {
        let panel = self.preview(drag)?;
        let (rect, (.., width, height)) = (panel.rect?, panel.screen?);
        let mut bgra = panel.renderer.as_ref()?.readback().ok()?;
        if bgra.len() != (width * height * 4) as usize {
            return None;
        }
        // Read back as RGBA, premultiplied.
        for pixel in bgra.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        Some(crate::native_drag::DragImage {
            width: width as u32,
            height: height as u32,
            bgra,
            scale: panel.scale,
            grab: (drag.x - rect.x, drag.y - rect.y),
        })
    }
    /// Show the drag preview, or not (`None`: Windows shows the drag's image
    /// meanwhile); shown again with the drag's pointer at `at` (the parent's
    /// coordinates).
    pub fn show_preview(
        &mut self,
        window: &winit::window::Window,
        drag: &zgui::input::DragState,
        at: Option<(f32, f32)>,
    ) {
        let Some(parent) = hwnd_of(window) else {
            return;
        };
        let Some(panel) = self.preview(drag) else {
            return;
        };
        match at {
            // SAFETY: hides our own window.
            None => unsafe {
                let _ = ShowWindow(panel.hwnd, SW_HIDE);
            },
            Some((x, y)) => {
                let Some(rect) = panel.rect else {
                    return;
                };
                let grab = (drag.x - rect.x, drag.y - rect.y);
                panel.place(
                    client_origin(parent),
                    Rect::new(x - grab.0, y - grab.1, rect.width, rect.height),
                );
                // SAFETY: shows our own window, never activating it.
                let _ = unsafe { ShowWindow(panel.hwnd, SW_SHOWNOACTIVATE) };
            }
        }
    }
    /// Hide every popup (the window closes or goes away).
    pub fn hide_all(&mut self, _: &winit::window::Window) {
        for (_, panel) in self.shown.drain(..) {
            release(panel);
        }
        self.forget_popups();
    }
    fn forget_popups(&self) {
        if let Some(parent) = self.parent {
            PARENTS.with(|parents| {
                if let Some(entry) = parents
                    .borrow_mut()
                    .iter_mut()
                    .find(|entry| entry.hwnd == parent)
                {
                    entry.popups.clear();
                }
            });
        }
    }
}
impl Drop for PopupHost {
    fn drop(&mut self) {
        for (_, panel) in self.shown.drain(..) {
            release(panel);
        }
        if let Some(parent) = self.parent {
            PARENTS.with(|parents| parents.borrow_mut().retain(|entry| entry.hwnd != parent));
        }
        HOSTS.set(HOSTS.get().saturating_sub(1));
        if HOSTS.get() == 0 {
            let pool = POOL.with(|pool| std::mem::take(&mut *pool.borrow_mut()));
            drop(pool);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zgui::scene::{Layout, QuadStyle, Style};

    /// A popup root holding one panel `size` points square, rounded `radius`.
    fn popup(size: f32, radius: f32) -> (Scene, NodeId) {
        let mut scene = Scene::new(400., 300.);
        let root = scene.append(
            scene.root(),
            NodeKind::Container(Layout::Column),
            Style::default(),
        );
        scene.append(
            root,
            NodeKind::Quad(QuadStyle {
                radius,
                ..Default::default()
            }),
            Style {
                width: Some(size),
                height: Some(size),
                ..Default::default()
            },
        );
        let _ = scene.flush();
        (scene, root)
    }

    #[test]
    fn a_rounded_panel_filling_its_window_takes_dwm_s_corners() {
        let window = Rect::new(0., 0., 100., 100.);
        let (scene, root) = popup(100., 8.);
        assert_eq!(corner_of(&scene, root, window), DWMWCP_ROUND);
        let (scene, root) = popup(100., 4.);
        assert_eq!(corner_of(&scene, root, window), DWMWCP_ROUNDSMALL);
        let (scene, root) = popup(100., 0.);
        assert_eq!(corner_of(&scene, root, window), DWMWCP_DONOTROUND);
        // Painting past its panel (a shadow of its own), zgui draws it all.
        let (scene, root) = popup(80., 8.);
        assert_eq!(corner_of(&scene, root, window), DWMWCP_DONOTROUND);
    }

    fn area(left: i32, top: i32, right: i32, bottom: i32) -> RECT {
        RECT {
            left,
            top,
            right,
            bottom,
        }
    }
    /// The popup zgui places in `screen` (window coordinates): flush with its
    /// right and bottom edges, as a clamped menu is.
    fn clamped(screen: Rect, width: f32, height: f32) -> Rect {
        Rect::new(
            screen.x + screen.width - width,
            screen.y + screen.height - height,
            width,
            height,
        )
    }

    #[test]
    fn a_popup_on_a_monitor_of_another_scale_stays_on_it() {
        // A 100% monitor, and a 150% one to its right, 2560 by 1400 pixels.
        let monitors = [area(0, 0, 1920, 1040), area(1920, 0, 4480, 1400)];
        // A window at 100%, its client at (100, 100), its trigger reaching
        // onto the 150% monitor: a popup clamped to that monitor's edges.
        let screens = screens_in((100, 100), 1., &monitors);
        assert_eq!(screens[1], Rect::new(1820., -100., 2560., 1400.));
        let popup = screen_rect((100, 100), 1., clamped(screens[1], 200., 300.));
        assert_eq!(popup, (4280, 1100, 200, 300));
        // A window at 150% on the right monitor, a popup clamped to the
        // 100% one's left and bottom edges: still inside it, its size the
        // window's 200 by 300 points at 150%.
        let screens = screens_in((2000, 100), 1.5, &monitors);
        let menu = Rect::new(
            screens[0].x,
            screens[0].y + screens[0].height - 300.,
            200.,
            300.,
        );
        let (left, top, width, height) = screen_rect((2000, 100), 1.5, menu);
        assert_eq!((left, top + height), (0, 1040));
        assert_eq!((width, height), (300, 450));
        assert!(left + width <= 1920);
    }

    #[test]
    fn a_popup_window_s_pixels_are_its_parent_s_points() {
        let target = Target::default();
        target.origin.set((16., 428.));
        target.scale.set(1.5);
        // Its client pixel (30, 45) is 20 by 30 points into the popup.
        assert_eq!(target.point((30, 45)), (36., 458.));
        // Dragged out above it (captured), past its edge.
        assert_eq!(target.point((-15, -30)), (6., 408.));
    }
}
