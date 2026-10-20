//! Clipboard selection follows the actual native display, never environment guesses.
//!
//! Wayland uses the application's connection and standard data-device protocol;
//! X11, macOS and Windows retain arboard. Construct before the first input event so the
//! Wayland worker can observe seat focus and the serial needed for selection.
use std::error::Error;
use winit::event_loop::OwnedDisplayHandle;
#[cfg(target_os = "linux")]
use winit::raw_window_handle::{HasDisplayHandle, RawDisplayHandle};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

enum Backend {
    Native(arboard::Clipboard),
    #[cfg(target_os = "linux")]
    Wayland(smithay_clipboard::Clipboard),
}

pub(crate) struct Clipboard {
    // Rust drops fields in declaration order. The worker must exit and join
    // before its borrowed wl_display connection is released by the owner.
    backend: Backend,
    _display: OwnedDisplayHandle,
}

impl Clipboard {
    pub(crate) fn new(display: OwnedDisplayHandle) -> Result<Self> {
        #[cfg(target_os = "linux")]
        if let RawDisplayHandle::Wayland(handle) = display.display_handle()?.as_raw() {
            // SAFETY: winit supplies a valid wl_display through its owned
            // display handle. `_display` retains the Wayland Connection for
            // the entire worker lifetime. `backend` drops first, and smithay's
            // Clipboard::drop joins its thread before returning. No raw pointer
            // escapes this module and the shared application owns this object.
            let backend = unsafe { smithay_clipboard::Clipboard::new(handle.display.as_ptr()) };
            return Ok(Self {
                backend: Backend::Wayland(backend),
                _display: display,
            });
        }
        Ok(Self {
            backend: Backend::Native(arboard::Clipboard::new()?),
            _display: display,
        })
    }

    pub(crate) fn get_text(&mut self) -> Result<String> {
        match &mut self.backend {
            Backend::Native(clipboard) => Ok(clipboard.get_text()?),
            #[cfg(target_os = "linux")]
            Backend::Wayland(clipboard) => Ok(clipboard.load()?),
        }
    }

    /// Wayland publication is asynchronous: success means the request was
    /// submitted, not that another process has accepted or persisted its data.
    /// The upstream API does not expose a publication acknowledgement.
    pub(crate) fn set_text(&mut self, text: String) -> Result<()> {
        match &mut self.backend {
            Backend::Native(clipboard) => clipboard.set_text(text)?,
            #[cfg(target_os = "linux")]
            Backend::Wayland(clipboard) => clipboard.store(text),
        }
        Ok(())
    }
}
