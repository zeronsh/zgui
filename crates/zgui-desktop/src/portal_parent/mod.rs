//! Native parent export; worker retains the parent until this guard is destroyed.
// Adapted from rfd 0.17.2 / ashpd. MIT; see LICENSE.
use std::fmt;
use winit::{
    raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle},
    window::Window,
};
mod wayland;
pub enum Parent {
    Wayland(wayland::WaylandWindowIdentifier),
    X11(u64),
}
#[derive(Debug)]
enum WindowIdentifierType {
    Wayland(String),
}
impl fmt::Display for WindowIdentifierType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Wayland(s) => write!(f, "wayland:{s}"),
        }
    }
}
impl fmt::Display for Parent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Wayland(p) => p.fmt(f),
            Self::X11(id) => write!(f, "x11:{id:x}"),
        }
    }
}
impl Parent {
    pub fn new(window: &Window) -> Option<Self> {
        match (
            window.window_handle().ok()?.as_raw(),
            window.display_handle().ok()?.as_raw(),
        ) {
            (RawWindowHandle::Xlib(w), _) => Some(Self::X11(w.window)),
            (RawWindowHandle::Xcb(w), _) => Some(Self::X11(w.window.get().into())),
            (RawWindowHandle::Wayland(w), RawDisplayHandle::Wayland(d)) => {
                // SAFETY: the window's live wl_surface, kept while `window`
                // is borrowed (the whole call), on winit's display. The
                // window does not keep the display connected: the event
                // loop (and after it the clipboard worker's handle) does,
                // so this relies on the app not ending its event loop in
                // the moment a dialog's worker exports its parent.
                // todo(wayland): an `OwnedDisplayHandle` for the worker.
                unsafe {
                    wayland::WaylandWindowIdentifier::from_raw(
                        w.surface.as_ptr(),
                        d.display.as_ptr(),
                    )
                    .map(Self::Wayland)
                }
            }
            _ => None,
        }
    }
}
