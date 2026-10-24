//! Windows' touch keyboard (the input pane): it comes up when a finger or a
//! pen puts the caret in an editor, and goes when no editor has the caret,
//! as for the system's own text fields. winit takes text through IMM32,
//! which Windows does not show the keyboard for by itself.
use windows::{
    UI::ViewManagement::InputPane,
    Win32::{Foundation::HWND, System::WinRT::IInputPaneInterop},
    core::factory,
};
use winit::{
    raw_window_handle::{HasWindowHandle, RawWindowHandle},
    window::Window,
};

fn pane(window: &Window) -> Option<InputPane> {
    let RawWindowHandle::Win32(handle) = window.window_handle().ok()?.as_raw() else {
        return None;
    };
    let interop = factory::<InputPane, IInputPaneInterop>().ok()?;
    // SAFETY: our own window, alive for the call.
    unsafe { interop.GetForWindow(HWND(handle.hwnd.get() as *mut _)) }.ok()
}

/// Show the touch keyboard over `window`; whether it shows. Windows keeps
/// it hidden while a hardware keyboard is attached, unless the user's
/// "Show the touch keyboard" setting says otherwise.
pub(crate) fn show(window: &Window) -> bool {
    pane(window).is_some_and(|pane| pane.TryShow().unwrap_or(false))
}

/// Put the touch keyboard away.
pub(crate) fn hide(window: &Window) {
    if let Some(pane) = pane(window) {
        let _ = pane.TryHide();
    }
}
